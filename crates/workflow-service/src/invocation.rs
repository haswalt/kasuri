//! Workspace-local child runs: pinned graphs, durable correlation and no HTTP dispatch.
use crate::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Parent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    pub run_id: String,
    pub node_id: String,
    pub root_id: String,
    pub depth: u32,
    pub awaited: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_index: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Child {
    #[serde(default)]
    pub collected: bool,
    pub node_id: String,
    pub run_id: String,
    pub workflow_id: String,
    pub revision: u32,
    pub awaited: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_index: Option<usize>,
    pub status: RunStatus,
}
pub(crate) fn contract(
    config: &serde_json::Map<String, Value>,
    key: &str,
    value: Option<&Value>,
) -> Result<(), ApiError> {
    if let Some(schema) = config.get(key) {
        let schema = json!({"schema":schema});
        let validator = connectors_core::validation::validate(schema.as_object().unwrap())
            .map_err(|_| error(400, "Invalid workflow input/output schema"))?;
        if value.is_some_and(|v| !validator.is_valid(v)) {
            return Err(error(
                400,
                "Workflow input/output does not match its contract",
            ));
        }
    }
    Ok(())
}
pub fn validate_node(node: &workflow_schema::Node, graph: &Workflow) -> Result<(), ApiError> {
    if node.kind == "Execute Workflow Trigger" {
        if node
            .config
            .keys()
            .any(|k| !matches!(k.as_str(), "inputSchema" | "outputSchema" | "outputNodeId"))
        {
            return Err(error(400, "Invalid invocation trigger configuration"));
        }
        let output = node
            .config
            .get("outputNodeId")
            .and_then(Value::as_str)
            .ok_or_else(|| error(400, "Choose a workflow output node"))?;
        if !graph
            .nodes
            .iter()
            .any(|n| n.id == output && !n.is_trigger())
            || graph.edges.iter().any(|e| e.source == output)
        {
            return Err(error(400, "Workflow output must be a terminal node"));
        }
        contract(&node.config, "inputSchema", None)?;
        contract(&node.config, "outputSchema", None)?;
    } else if node.kind == "Each / Loop" {
        crate::items::validate(node)?;
    } else {
        if node.config.keys().any(|k| {
            !matches!(
                k.as_str(),
                "workflowId" | "revision" | "mode" | "inputPointer"
            )
        }) || node
            .config
            .get("workflowId")
            .and_then(Value::as_str)
            .is_none_or(|id| id.is_empty() || id.len() > 256)
            || !matches!(
                node.config
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or("await"),
                "await" | "background"
            )
            || node.config.get("mode").is_some_and(|v| !v.is_string())
            || node
                .config
                .get("revision")
                .is_some_and(|v| v.as_u64().is_none_or(|v| v == 0 || v > u32::MAX as u64))
            || node.config.get("inputPointer").is_some_and(|v| {
                !v.is_string()
                    || (!v.as_str().unwrap().is_empty() && !v.as_str().unwrap().starts_with('/'))
            })
        {
            return Err(error(400, "Invalid Execute Workflow configuration"));
        }
    }
    Ok(())
}
pub(crate) fn target_key(node: &workflow_schema::Node) -> String {
    json!([node.config.get("workflowId"), node.config.get("revision")]).to_string()
}
impl Workspace {
    /// Permission checks and execution expand the same synchronous published graph set.
    pub fn invocation_targets(
        &self,
        graph: &Workflow,
    ) -> Result<BTreeMap<String, Workflow>, ApiError> {
        fn visit(
            ws: &Workspace,
            graph: &Workflow,
            path: &mut BTreeSet<String>,
            targets: &mut BTreeMap<String, Workflow>,
            depth: usize,
        ) -> Result<(), ApiError> {
            if depth > 4 {
                return Err(error(400, "Workflow invocation depth exceeds four"));
            }
            if !path.insert(graph.id.clone()) {
                return Err(error(400, "Recursive workflow invocation is not supported"));
            }
            for node in graph
                .nodes
                .iter()
                .filter(|n| matches!(n.kind.as_str(), "Execute Workflow" | "Each / Loop"))
            {
                validate_node(node, graph)?;
                let id = node.config["workflowId"].as_str().unwrap();
                let mut doc = ws.live_document(id)?;
                if doc.status != Publication::Active {
                    return Err(error(409, "Invoked workflow must be active"));
                }
                if let Some(revision) = node.config.get("revision").and_then(Value::as_u64) {
                    doc.graph = ws
                        .revisions
                        .iter()
                        .find(|r| r.workflow_id == id && r.revision as u64 == revision)
                        .ok_or_else(|| error(404, "Requested child revision not found"))?
                        .graph
                        .clone()
                }
                Workspace::executable(&doc)?;
                if !doc
                    .graph
                    .nodes
                    .iter()
                    .any(|n| n.kind == "Execute Workflow Trigger")
                {
                    return Err(error(400, "Target needs an Execute Workflow Trigger"));
                }
                visit(ws, &doc.graph, path, targets, depth + 1)?;
                targets.insert(target_key(node), doc.graph);
                if targets.len() > 16 {
                    return Err(error(400, "Too many invocation targets"));
                }
            }
            path.remove(&graph.id);
            Ok(())
        }
        let mut targets = BTreeMap::new();
        visit(self, graph, &mut BTreeSet::new(), &mut targets, 0)?;
        Ok(targets)
    }
    pub(crate) fn run_index(&self, id: &str) -> Option<usize> {
        self.runs
            .iter()
            .position(|r| r.restore().is_ok_and(|r| r.id() == id))
    }
    pub(crate) fn invocation_protected(&self, index: usize) -> bool {
        let Ok(run) = self.runs[index].restore() else {
            return true;
        };
        self.runs.iter().any(|r| {
            r.status()
                .is_ok_and(|s| matches!(s, RunStatus::Running | RunStatus::Paused))
                && (r
                    .parent
                    .as_ref()
                    .is_some_and(|p| p.root_id == run.id() || p.run_id == run.id())
                    || r.children
                        .iter()
                        .any(|c| c.awaited && !c.collected && c.run_id == run.id()))
        })
    }
    pub(crate) fn child_output(record: &RunRecord) -> Result<Value, ApiError> {
        let run = record.restore()?;
        let trigger = run
            .workflow()
            .nodes
            .iter()
            .find(|n| n.kind == "Execute Workflow Trigger")
            .ok_or_else(|| error(500, "Missing child trigger"))?;
        let output = trigger.config["outputNodeId"]
            .as_str()
            .and_then(|id| run.nodes().get(id))
            .and_then(|s| {
                if let NodeState::Succeeded { output, .. } = s {
                    Some(output.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| error(400, "Child output was not reached"))?;
        contract(&trigger.config, "outputSchema", Some(&output))?;
        Ok(output)
    }
    pub(crate) fn cancel_child(&mut self, id: &str) -> Result<bool, ApiError> {
        let Some(index) = self.run_index(id) else {
            return Ok(false);
        };
        let record = &mut self.runs[index];
        if !record
            .status()
            .is_ok_and(|s| matches!(s, RunStatus::Running | RunStatus::Paused))
        {
            return Ok(false);
        }
        let mut run = record.restore()?;
        run.cancel().map_err(|e| error(500, e.to_string()))?;
        record.checkpoint = run.checkpoint().map_err(|e| error(500, e.to_string()))?;
        record.external = None;
        record.pending.clear();
        record.wake_at = None;
        Ok(true)
    }
    /// All transitions commit with the workspace before any child network/data effect.
    pub fn settle_invocations(&mut self, now: u64) -> Result<(), ApiError> {
        let item_work = self.runs.iter().any(|r| {
            r.external
                .iter()
                .chain(r.pending.iter())
                .any(|e| e.task.node.kind == "Each / Loop")
        });
        // Bound synchronous collection work; remaining progress is alarm-backed.
        for _ in 0..if item_work { 8 } else { 128 } {
            let mut changed = false;
            for record in &mut self.runs {
                if record.parent.is_some()
                    && record.status().is_ok_and(|s| s == RunStatus::Succeeded)
                    && Self::child_output(record).is_err()
                {
                    record.host_error = Some("invalid_child_output".into());
                    changed = true;
                }
            }
            let cancelled: Vec<String> = self
                .runs
                .iter()
                .flat_map(|r| {
                    let terminal = r
                        .status()
                        .is_ok_and(|s| matches!(s, RunStatus::Failed | RunStatus::Cancelled));
                    let run = r.restore().ok();
                    r.children
                        .iter()
                        .filter(move |c| {
                            c.awaited
                                && (terminal
                                    || run.as_ref().is_some_and(|run| {
                                        !matches!(
                                            run.nodes().get(&c.node_id),
                                            Some(NodeState::Running { .. })
                                        )
                                    }))
                        })
                        .map(|c| c.run_id.clone())
                })
                .collect();
            for id in cancelled {
                changed |= self.cancel_child(&id)?;
            }
            let pending: Vec<(String, Task)> = self
                .runs
                .iter()
                .filter(|r| r.status().is_ok_and(|s| s == RunStatus::Running))
                .flat_map(|r| {
                    let id = r.restore().unwrap().id().to_string();
                    r.external
                        .iter()
                        .chain(r.pending.iter())
                        .filter(|e| e.task.node.kind == "Execute Workflow")
                        .map(move |e| (id.clone(), e.task.clone()))
                })
                .collect();
            for (id, task) in pending {
                let index = self.run_index(&id).unwrap();
                if !self.runs[index]
                    .status()
                    .is_ok_and(|s| s == RunStatus::Running)
                    || !self.runs[index]
                        .external
                        .iter()
                        .chain(self.runs[index].pending.iter())
                        .any(|e| e.task.id == task.id)
                {
                    continue;
                }

                // Virtual completion times cannot expire unrelated live runs.
                let time = if self.runs[index].mode == Mode::Simulation {
                    self.runs[index].logical_clock
                } else {
                    now
                };
                let existing = self.runs[index]
                    .children
                    .iter()
                    .find(|c| c.node_id == task.node.id)
                    .cloned();
                let child = match existing {
                    Some(child) => child,
                    None => match self.create_child(index, &task, time) {
                        Ok(child) => {
                            changed = true;
                            child
                        }
                        Err(_) => {
                            self.finish_invocation(
                                &id,
                                &task,
                                connectors_http::failure(FailureCode::InvocationFailed, false),
                                time,
                            )?;
                            changed = true;
                            continue;
                        }
                    },
                };
                let child_index = self.run_index(&child.run_id);
                let status = child_index
                    .map(|i| self.runs[i].status())
                    .transpose()?
                    .unwrap_or(RunStatus::Failed);
                if !child.awaited {
                    self.finish_invocation(&id,&task,Outcome::success(json!({"runId":child.run_id,"workflowId":child.workflow_id,"revision":child.revision,"status":status})),time)?;
                    changed = true;
                } else if status == RunStatus::Succeeded {
                    let record = &self.runs[child_index.unwrap()];
                    let time = if record.mode == Mode::Simulation {
                        time.max(record.logical_clock)
                    } else {
                        time
                    };
                    let outcome = match Self::child_output(record) {
                        Ok(output) => Outcome::success(
                            json!({"runId":child.run_id,"workflowId":child.workflow_id,"revision":child.revision,"status":status,"output":output}),
                        ),
                        Err(_) => connectors_http::failure(FailureCode::InvocationFailed, false),
                    };
                    self.finish_invocation(&id, &task, outcome, time)?;
                    changed = true;
                } else if matches!(status, RunStatus::Failed | RunStatus::Cancelled)
                    || time >= task.deadline_ms
                {
                    self.cancel_child(&child.run_id)?;
                    self.finish_invocation(
                        &id,
                        &task,
                        connectors_http::failure(FailureCode::InvocationFailed, false),
                        time,
                    )?;
                    changed = true;
                } else {
                    let wake = child_index
                        .and_then(|i| self.runs[i].wake_at)
                        .unwrap_or(task.deadline_ms)
                        .min(task.deadline_ms);
                    let index = self.run_index(&id).unwrap();
                    self.runs[index].wake_at = Some(wake.max(now.saturating_add(1)));
                }
            }
            changed |= self.settle_items(now)?;
            if !changed {
                break;
            }
        }
        let statuses: BTreeMap<String, RunStatus> = self
            .runs
            .iter()
            .filter_map(|r| Some((r.restore().ok()?.id().into(), r.status().ok()?)))
            .collect();
        for record in &mut self.runs {
            for child in &mut record.children {
                if let Some(status) = statuses.get(&child.run_id) {
                    child.status = *status
                }
            }
        }
        Ok(())
    }
    pub(crate) fn finish_invocation(
        &mut self,
        id: &str,
        task: &Task,
        outcome: Outcome,
        now: u64,
    ) -> Result<(), ApiError> {
        let index = self.run_index(id).unwrap();
        let record = &mut self.runs[index];
        let now = if record.mode == Mode::Simulation {
            now.max(record.logical_clock)
        } else {
            now
        };
        let mut run = record.restore()?;
        run.complete(&task.id, outcome, now)
            .map_err(|e| error(500, e.to_string()))?;
        record.checkpoint = run.checkpoint().map_err(|e| error(500, e.to_string()))?;
        if record
            .external
            .as_ref()
            .is_some_and(|e| e.task.id == task.id)
        {
            record.external = None;
        }
        record.pending.retain(|e| e.task.id != task.id);
        record.logical_clock = record.logical_clock.max(now);
        record.advance(now)
    }
    fn create_child(&mut self, index: usize, task: &Task, now: u64) -> Result<Child, ApiError> {
        self.create_item_child(index, task, now, None)
    }
    pub(crate) fn create_item_child(
        &mut self,
        index: usize,
        task: &Task,
        now: u64,
        item_index: Option<usize>,
    ) -> Result<Child, ApiError> {
        let record = &self.runs[index];
        let parent_run = record.restore()?;
        let graph = record
            .invocation_targets
            .get(&target_key(&task.node))
            .cloned()
            .ok_or_else(|| error(409, "Pinned child is unavailable"))?;
        let input = task
            .input
            .pointer(
                task.node
                    .config
                    .get("inputPointer")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            )
            .cloned()
            .ok_or_else(|| error(400, "Child input is missing"))?;
        let trigger = graph
            .nodes
            .iter()
            .find(|n| n.kind == "Execute Workflow Trigger")
            .unwrap();
        contract(&trigger.config, "inputSchema", Some(&input))?;
        let depth = record.parent.as_ref().map_or(1, |p| p.depth + 1);
        let root_id = record
            .parent
            .as_ref()
            .map_or_else(|| parent_run.id().to_string(), |p| p.root_id.clone());
        let root_index = self
            .run_index(&root_id)
            .ok_or_else(|| error(409, "Invocation root is unavailable"))?;
        if depth > 4
            || self.runs[root_index].child_count
                >= if self.runs[root_index]
                    .invocation_targets
                    .values()
                    .chain(std::iter::once(self.runs[root_index].restore()?.workflow()))
                    .any(|g| g.nodes.iter().any(|n| n.kind == "Each / Loop"))
                {
                    256
                } else {
                    16
                }
            || self
                .runs
                .iter()
                .filter(|r| {
                    r.status()
                        .is_ok_and(|s| matches!(s, RunStatus::Running | RunStatus::Paused))
                })
                .count()
                >= 5
        {
            return Err(error(429, "Child run budget exceeded"));
        }
        let id = format!(
            "child-{:x}",
            Sha256::digest(task.idempotency_key.as_bytes())
        );
        let awaited = task.node.config.get("mode").and_then(Value::as_str) != Some("background");
        let time = if record.mode == Mode::Simulation {
            record.logical_clock
        } else {
            now
        };
        let mode = if record.mode == Mode::Simulation {
            Mode::Simulation
        } else {
            Mode::Run
        };
        let run = Run::new(&id, graph.clone(), input, time, Limits::default())
            .map_err(|e| error(400, e.to_string()))?;
        let parent = Parent {
            run_id: parent_run.id().into(),
            node_id: task.node.id.clone(),
            root_id,
            depth,
            awaited,
            item_index,
            item_id: item_index
                .map(|_| format!("item-{:x}", Sha256::digest(task.idempotency_key.as_bytes()))),
        };
        let mut child_record = RunRecord {
            invocation_targets: record.invocation_targets.clone(),
            parent: Some(parent.clone()),
            children: vec![],
            iterations: BTreeMap::new(),
            child_count: 0,
            external: None,
            pending: Vec::new(),
            logical_clock: time,
            trigger_context: json!({"trigger":"workflow","parent":parent}),
            webhook_response: None,
            checkpoint: run.checkpoint().map_err(|e| error(400, e.to_string()))?,
            mode,
            created_at: time,
            wake_at: None,
            host_error: None,
        };
        child_record.advance(time)?;
        let child = Child {
            node_id: task.node.id.clone(),
            run_id: id,
            workflow_id: graph.id,
            revision: graph.revision,
            awaited,
            status: child_record.status()?,
            item_index,
            collected: false,
        };
        self.runs[root_index].child_count += 1;
        self.runs[index].children.push(child.clone());
        self.runs.insert(0, child_record);
        Ok(child)
    }
}
