//! Deterministic bounded DAG scheduler. A host persists each checkpoint before dispatching
//! its task, executes connectors, then persists the reported outcome before advancing.
//! This crate does not itself provide durable storage or exactly-once side effects.
use connector_sdk::{Failure, FailureCode, Outcome, Task};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use workflow_schema::Workflow;

pub mod compact;
pub const CHECKPOINT_VERSION: u32 = 1;
/// A checkpoint's full run state as JSON, whether it is stored compactly or not.
/// Read checkpoints through this (or [`Run::restore`]), never as raw JSON.
pub fn checkpoint_state(text: &str) -> Result<Value, EngineError> {
    compact::decode(text, MAX_CHECKPOINT_BYTES).map_err(|e| error(&e))
}
/// Encoded JSON size without building the string.
fn encoded_len(value: &Value) -> usize {
    serialized_len(value)
}
fn serialized_len(value: &impl Serialize) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    let _ = serde_json::to_writer(&mut count, value);
    count.0
}
const MAX_CHECKPOINT_BYTES: usize = 2_097_152;
const MAX_VALUE_BYTES: usize = 65_536;
/// Legacy graphs retain one in-flight slot. Explicit split markers opt into a bound.
pub fn concurrency(workflow: &Workflow) -> usize {
    workflow
        .nodes
        .iter()
        .filter(|n| n.kind == "Split / Parallel")
        .map(|n| {
            n.config
                .get("concurrency")
                .and_then(|v| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|v| v.parse().ok()))
                })
                .filter(|v| matches!(v, 2 | 4 | 8))
                .unwrap_or(4) as usize
        })
        .min()
        .unwrap_or(1)
}

fn collected_failure(workflow: &Workflow, node_id: &str) -> bool {
    let outgoing: Vec<_> = workflow
        .edges
        .iter()
        .filter(|e| e.source == node_id)
        .collect();
    !outgoing.is_empty()
        && outgoing.iter().all(|e| {
            workflow.nodes.iter().any(|n| {
                n.id == e.target
                    && n.kind == "Join"
                    && n.config.get("policy").and_then(Value::as_str) == Some("collect")
            })
        })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub max_dispatches: u32,
    pub max_duration_ms: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_dispatches: 1_000,
            max_duration_ms: 7 * 24 * 60 * 60 * 1_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Paused,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum NodeState {
    Pending,
    Running {
        task: Task,
    },
    Waiting {
        until_ms: u64,
        output: Value,
        attempts: u32,
    },
    Retry {
        at_ms: u64,
        attempts: u32,
    },
    Succeeded {
        output: Value,
        port: String,
        attempts: u32,
    },
    Skipped,
    Failed {
        failure: Failure,
        attempts: u32,
    },
}
impl NodeState {
    fn settled(&self) -> bool {
        matches!(
            self,
            Self::Succeeded { .. } | Self::Skipped | Self::Failed { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Dispatch {
    Task(Box<Task>),
    SleepUntil(u64),
    Paused,
    Finished(RunStatus),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineError(pub String);
impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for EngineError {}
fn error(message: &str) -> EngineError {
    EngineError(message.into())
}
fn check_value(value: &Value) -> Result<(), EngineError> {
    // Measured without building the encoded copy.
    if encoded_len(value) > MAX_VALUE_BYTES {
        return Err(error(
            "Input/output exceeds 64 KiB; store large artifacts externally",
        ));
    }
    Ok(())
}

/// Serialized state contains workflow and data snapshots. Keep it in authenticated,
/// access-controlled storage; callers must redact sensitive connector outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    checkpoint_version: u32,
    id: String,
    workflow: Workflow,
    input: Value,
    status: RunStatus,
    nodes: BTreeMap<String, NodeState>,
    limits: Limits,
    started_at_ms: u64,
    observed_at_ms: u64,
    dispatches: u32,
    failure: Option<FailureCode>,
}

impl Run {
    /// Only graph ancestors are visible, including skipped ancestors as missing values.
    fn expression_context(&self, node: &workflow_schema::Node) -> Value {
        let mut ancestors = std::collections::BTreeSet::new();
        let mut pending = vec![node.id.clone()];
        while let Some(id) = pending.pop() {
            for edge in self.workflow.edges.iter().filter(|e| e.target == id) {
                if ancestors.insert(edge.source.clone()) {
                    pending.push(edge.source.clone());
                }
            }
        }
        fn references(
            value: &Value,
            ids: &mut std::collections::BTreeSet<String>,
            trigger: &mut bool,
        ) {
            if let Some(o) = value.as_object() {
                if o.get("op").and_then(Value::as_str) == Some("literal") {
                    return;
                }
                if o.get("op").and_then(Value::as_str) == Some("node") {
                    if let Some(id) = o.get("nodeId").and_then(Value::as_str) {
                        ids.insert(id.into());
                    }
                }
                if o.get("op").and_then(Value::as_str) == Some("trigger") {
                    *trigger = true;
                }
                for v in o.values() {
                    references(v, ids, trigger);
                }
            } else if let Some(a) = value.as_array() {
                for v in a {
                    references(v, ids, trigger);
                }
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut trigger = false;
        if node.kind == "Expressions" {
            references(&Value::Object(node.config.clone()), &mut ids, &mut trigger);
        }
        references(
            &json!({"fields":node.input_bindings}),
            &mut ids,
            &mut trigger,
        );
        let outputs: serde_json::Map<String, Value> = ids
            .intersection(&ancestors)
            .filter_map(|id| {
                if let Some(NodeState::Succeeded { output, .. }) = self.nodes.get(id) {
                    Some((id.clone(), output.clone()))
                } else {
                    None
                }
            })
            .collect();
        let mut context = json!({"nodes":outputs,"run":{"id":self.id,"startedAtMs":self.started_at_ms},"workflow":{"id":self.workflow.id,"name":self.workflow.name,"revision":self.workflow.revision}});
        if trigger {
            context["trigger"] = self.input.clone();
        }
        context
    }
    pub fn new(
        id: impl Into<String>,
        workflow: Workflow,
        input: Value,
        now_ms: u64,
        limits: Limits,
    ) -> Result<Self, EngineError> {
        workflow
            .validate()
            .map_err(|e| EngineError(e.to_string()))?;
        check_value(&input)?;
        let id = id.into();
        if id.trim().is_empty() || id.len() > 256 {
            return Err(error("Run ID must be 1–256 bytes"));
        }
        if limits.max_dispatches == 0
            || limits.max_duration_ms == 0
            || now_ms.checked_add(limits.max_duration_ms).is_none()
        {
            return Err(error("Invalid execution limits"));
        }
        let nodes = workflow
            .nodes
            .iter()
            .map(|n| (n.id.clone(), NodeState::Pending))
            .collect();
        let run = Self {
            checkpoint_version: CHECKPOINT_VERSION,
            id,
            workflow,
            input,
            status: RunStatus::Running,
            nodes,
            limits,
            started_at_ms: now_ms,
            observed_at_ms: now_ms,
            dispatches: 0,
            failure: None,
        };
        run.check_size()?;
        Ok(run)
    }
    /// The immutable original trigger input, separate from intermediate outputs.
    pub fn input(&self) -> &Value {
        &self.input
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn status(&self) -> RunStatus {
        self.status
    }
    pub fn nodes(&self) -> &BTreeMap<String, NodeState> {
        &self.nodes
    }
    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }
    pub fn failure(&self) -> Option<&FailureCode> {
        self.failure.as_ref()
    }
    pub fn dispatches(&self) -> u32 {
        self.dispatches
    }

    /// The run's state, with repeated large values stored once (see [`compact`]).
    /// The 2 MiB budget applies to the full, expanded state.
    pub fn checkpoint(&self) -> Result<String, EngineError> {
        self.check_size()?;
        let state = serde_json::to_value(self).map_err(|_| error("Cannot serialize checkpoint"))?;
        Ok(compact::encode(state))
    }
    /// Enforce the 2 MiB budget on the full state without encoding a checkpoint.
    fn check_size(&self) -> Result<(), EngineError> {
        if serialized_len(self) > MAX_CHECKPOINT_BYTES {
            return Err(error("Checkpoint exceeds 2 MiB; reduce output sizes"));
        }
        Ok(())
    }

    /// Restore only host-owned checkpoints, never arbitrary user-provided run state.
    pub fn restore(text: &str) -> Result<Self, EngineError> {
        if text.len() > MAX_CHECKPOINT_BYTES {
            return Err(error("Checkpoint exceeds size limit"));
        }
        let run: Self = if compact::is_compact_text(text) {
            let state = compact::decode(text, MAX_CHECKPOINT_BYTES).map_err(|e| error(&e))?;
            if encoded_len(&state) > MAX_CHECKPOINT_BYTES {
                return Err(error("Checkpoint exceeds size limit"));
            }
            serde_json::from_value(state).map_err(|_| error("Invalid checkpoint JSON"))?
        } else {
            serde_json::from_str(text).map_err(|_| error("Invalid checkpoint JSON"))?
        };
        if run.checkpoint_version != CHECKPOINT_VERSION {
            return Err(error("Unsupported checkpoint version"));
        }
        run.workflow
            .validate()
            .map_err(|e| EngineError(e.to_string()))?;
        if run.id.trim().is_empty()
            || run.id.len() > 256
            || run.limits.max_dispatches == 0
            || run.limits.max_duration_ms == 0
            || run
                .started_at_ms
                .checked_add(run.limits.max_duration_ms)
                .is_none()
            || run.observed_at_ms < run.started_at_ms
            || run.dispatches > run.limits.max_dispatches
        {
            return Err(error("Invalid checkpoint limits or timestamps"));
        }
        check_value(&run.input)?;
        if run.nodes.len() != run.workflow.nodes.len()
            || run
                .nodes
                .values()
                .filter(|s| matches!(s, NodeState::Running { .. }))
                .count()
                > concurrency(&run.workflow)
        {
            return Err(error("Invalid checkpoint node set"));
        }
        for node in &run.workflow.nodes {
            let state = run
                .nodes
                .get(&node.id)
                .ok_or_else(|| error("Checkpoint is missing a node"))?;
            let attempts = match state {
                NodeState::Running { task } => {
                    if task.node != *node
                        || task.id != run.task_id(&node.id, task.attempt)
                        || task.idempotency_key != run.effect_key(&node.id)
                        || task.deadline_ms <= run.started_at_ms
                        || task.deadline_ms > run.deadline()
                    {
                        return Err(error("Checkpoint task does not match its node"));
                    }
                    check_value(&task.input)?;
                    Some(task.attempt)
                }
                NodeState::Waiting {
                    output, attempts, ..
                } => {
                    check_value(output)?;
                    Some(*attempts)
                }
                NodeState::Succeeded {
                    output,
                    port,
                    attempts,
                } => {
                    check_value(output)?;
                    if !Self::valid_port(node, port) {
                        return Err(error("Invalid checkpoint output port"));
                    }
                    Some(*attempts)
                }
                NodeState::Retry { attempts, .. } | NodeState::Failed { attempts, .. } => {
                    Some(*attempts)
                }
                _ => None,
            };
            if attempts.is_some_and(|a| a == 0 || a > node.retry.max_attempts || a > run.dispatches)
            {
                return Err(error("Invalid checkpoint attempt count"));
            }
            if matches!(state, NodeState::Retry { attempts, .. } if *attempts >= node.retry.max_attempts)
            {
                return Err(error("Checkpoint retry exhausted"));
            }
        }
        if run.status == RunStatus::Succeeded
            && !run.nodes.iter().all(|(id, s)| {
                matches!(s, NodeState::Succeeded { .. } | NodeState::Skipped)
                    || (matches!(s, NodeState::Failed { .. })
                        && collected_failure(&run.workflow, id))
            })
        {
            return Err(error("Successful checkpoint has unfinished nodes"));
        }
        Ok(run)
    }

    pub fn pause(&mut self) -> Result<(), EngineError> {
        if self.status != RunStatus::Running {
            return Err(error("Only a running run can pause"));
        }
        self.status = RunStatus::Paused;
        Ok(())
    }
    pub fn resume(&mut self) -> Result<(), EngineError> {
        if self.status != RunStatus::Paused {
            return Err(error("Only a paused run can resume"));
        }
        self.status = RunStatus::Running;
        Ok(())
    }
    pub fn cancel(&mut self) -> Result<(), EngineError> {
        if !matches!(self.status, RunStatus::Running | RunStatus::Paused) {
            return Err(error("Run is already terminal"));
        }
        self.status = RunStatus::Cancelled;
        Ok(())
    }
    fn observe(&mut self, now_ms: u64) -> Result<(), EngineError> {
        if now_ms < self.observed_at_ms {
            return Err(error("Host clock moved backwards"));
        }
        self.observed_at_ms = now_ms;
        Ok(())
    }
    fn deadline(&self) -> u64 {
        self.started_at_ms + self.limits.max_duration_ms
    }
    fn terminate(&mut self, code: FailureCode) -> Dispatch {
        self.status = RunStatus::Failed;
        self.failure = Some(code);
        Dispatch::Finished(self.status)
    }
    // JSON tuples prevent collisions when user-supplied IDs contain separators.
    fn effect_key(&self, node_id: &str) -> String {
        json!([self.id, node_id]).to_string()
    }
    fn task_id(&self, node_id: &str, attempt: u32) -> String {
        json!([self.id, node_id, attempt]).to_string()
    }
    fn valid_port(node: &workflow_schema::Node, port: &str) -> bool {
        if node.kind == "Fail Workflow" {
            false
        } else if node.kind == "IF / Condition" {
            matches!(port, "true" | "false" | "error")
        } else if node.kind == "Switch / Router" {
            matches!(port, "fallback" | "error")
                || node
                    .config
                    .get("rules")
                    .and_then(Value::as_array)
                    .is_some_and(|rules| {
                        rules
                            .iter()
                            .any(|r| r.get("port").and_then(Value::as_str) == Some(port))
                    })
        } else {
            matches!(port, "success" | "error")
        }
    }

    /// Select at most one task. Repeated calls while it is in flight return the same task.
    /// All joins wait for every parent to settle; their input is keyed by active parent ID.
    pub fn next(&mut self, now_ms: u64) -> Result<Dispatch, EngineError> {
        // Apply each state transition atomically, including checkpoint budget enforcement.
        let mut candidate = self.clone();
        let dispatch = candidate.advance(now_ms, false)?;
        candidate.check_size()?;
        *self = candidate;
        Ok(dispatch)
    }
    /// Select a new task without redispatching an already running sibling.
    pub fn next_parallel(&mut self, now_ms: u64) -> Result<Dispatch, EngineError> {
        let mut candidate = self.clone();
        let dispatch = candidate.advance(now_ms, true)?;
        candidate.check_size()?;
        *self = candidate;
        Ok(dispatch)
    }
    fn advance(&mut self, now_ms: u64, parallel: bool) -> Result<Dispatch, EngineError> {
        self.observe(now_ms)?;
        if !matches!(self.status, RunStatus::Running | RunStatus::Paused) {
            return Ok(Dispatch::Finished(self.status));
        }
        if now_ms >= self.deadline() {
            return Ok(self.terminate(FailureCode::DeadlineExceeded));
        }
        if self.status == RunStatus::Paused {
            return Ok(Dispatch::Paused);
        }
        let running: Vec<Task> = self
            .nodes
            .values()
            .filter_map(|s| {
                if let NodeState::Running { task } = s {
                    Some(task.clone())
                } else {
                    None
                }
            })
            .collect();
        for task in running {
            if now_ms < task.deadline_ms {
                if !parallel {
                    return Ok(Dispatch::Task(Box::new(task)));
                }
            } else {
                self.apply(
                    &task.id,
                    Outcome::Failed {
                        failure: Failure::new(FailureCode::Timeout, true),
                    },
                    now_ms,
                )?;
                if self.status == RunStatus::Failed {
                    return Ok(Dispatch::Finished(self.status));
                }
            }
        }
        for state in self.nodes.values_mut() {
            if let NodeState::Waiting {
                until_ms,
                output,
                attempts,
            } = state
                && *until_ms <= now_ms
            {
                *state = NodeState::Succeeded {
                    output: output.clone(),
                    port: "success".into(),
                    attempts: *attempts,
                };
            }
        }
        let slots_full = parallel
            && self
                .nodes
                .values()
                .filter(|s| matches!(s, NodeState::Running { .. }))
                .count()
                >= concurrency(&self.workflow);
        // Iterate to propagate skipped branches even when nodes are not topologically ordered.
        loop {
            let mut skipped = false;
            for node in &self.workflow.nodes {
                let attempt = match self.nodes[&node.id] {
                    NodeState::Pending => 1,
                    NodeState::Retry { at_ms, attempts } if at_ms <= now_ms => attempts + 1,
                    _ => continue,
                };
                let parents: Vec<_> = self
                    .workflow
                    .edges
                    .iter()
                    .filter(|e| e.target == node.id)
                    .collect();
                if parents.iter().any(|e| !self.nodes[&e.source].settled()) {
                    continue;
                }
                let active: Vec<_> = parents
                    .iter()
                    .filter_map(|e| match &self.nodes[&e.source] {
                        NodeState::Succeeded { output, port, .. } if *port == e.source_port => {
                            Some((e.source.clone(), output.clone()))
                        }
                        _ => None,
                    })
                    .collect();
                if !parents.is_empty()
                    && active.is_empty()
                    && !(node.kind == "Join"
                        && node.config.get("policy").and_then(Value::as_str) == Some("collect")
                        && parents
                            .iter()
                            .any(|e| matches!(self.nodes[&e.source], NodeState::Failed { .. })))
                {
                    self.nodes.insert(node.id.clone(), NodeState::Skipped);
                    skipped = true;
                    continue;
                }
                if slots_full {
                    continue;
                }
                if self.dispatches >= self.limits.max_dispatches {
                    return Ok(self.terminate(FailureCode::BudgetExceeded));
                }
                let input = if node.kind == "Join"
                    && node.config.get("policy").and_then(Value::as_str) == Some("collect")
                {
                    Value::Object(
                        parents
                            .iter()
                            .map(|edge| {
                                let entry = match &self.nodes[&edge.source] {
                                    NodeState::Succeeded { output, port, .. }
                                        if *port == edge.source_port =>
                                    {
                                        json!({"status":"succeeded","output":output})
                                    }
                                    NodeState::Failed { failure, .. } => {
                                        json!({"status":"failed","error":failure.code})
                                    }
                                    _ => json!({"status":"skipped"}),
                                };
                                (edge.source.clone(), entry)
                            })
                            .collect(),
                    )
                } else if node.kind == "Join" || (node.kind == "Merge" && !node.config.is_empty()) {
                    Value::Object(active.into_iter().collect())
                } else if parents.is_empty() {
                    self.input.clone()
                } else if parents.len() == 1 {
                    active[0].1.clone()
                } else {
                    Value::Object(active.into_iter().collect())
                };
                check_value(&input)?;
                let mut task = Task {
                    id: self.task_id(&node.id, attempt),
                    idempotency_key: self.effect_key(&node.id),
                    node: node.clone(),
                    input,
                    context: if matches!(node.kind.as_str(), "Expressions" | "Sentry")
                        || !node.input_bindings.is_empty()
                    {
                        self.expression_context(node)
                    } else {
                        Value::Null
                    },
                    attempt,
                    deadline_ms: now_ms.saturating_add(node.timeout_ms).min(self.deadline()),
                };
                self.dispatches += 1;
                if !node.input_bindings.is_empty() {
                    match workflow_expressions::assemble(&node.input_bindings, &task) {
                        Ok(input) => {
                            task.input = input;
                            // Bindings are fully resolved here. Only the Expressions
                            // connector evaluates its own config against the context, so
                            // other connectors must not receive or persist ancestor
                            // outputs or the trigger payload.
                            if node.kind != "Expressions" {
                                task.context = Value::Null;
                            }
                        }
                        Err(code) => {
                            self.nodes
                                .insert(node.id.clone(), NodeState::Running { task: task.clone() });
                            self.apply(
                                &task.id,
                                Outcome::Failed {
                                    failure: Failure::permanent(code),
                                },
                                now_ms,
                            )?;
                            return self.advance(now_ms, parallel);
                        }
                    }
                }
                self.nodes
                    .insert(node.id.clone(), NodeState::Running { task: task.clone() });
                return Ok(Dispatch::Task(Box::new(task)));
            }
            if !skipped {
                break;
            }
        }
        if self.nodes.values().all(NodeState::settled) {
            self.status = RunStatus::Succeeded;
            return Ok(Dispatch::Finished(self.status));
        }
        let wake_at = self
            .nodes
            .values()
            .filter_map(|s| match s {
                NodeState::Waiting { until_ms, .. } => Some(*until_ms),
                NodeState::Retry { at_ms, .. } if !slots_full || *at_ms > now_ms => Some(*at_ms),
                NodeState::Running { task } if parallel => Some(task.deadline_ms),
                _ => None,
            })
            .min();
        match wake_at {
            Some(at_ms) => Ok(Dispatch::SleepUntil(at_ms.min(self.deadline()))),
            None => Err(error("No runnable nodes or scheduled wake-up")),
        }
    }

    /// Reject stale/duplicate completions. A completion during pause is retained, but no
    /// subsequent task is dispatched until resume. Hosts must serialize access per run.
    pub fn complete(
        &mut self,
        task_id: &str,
        outcome: Outcome,
        now_ms: u64,
    ) -> Result<(), EngineError> {
        let mut candidate = self.clone();
        candidate.apply(task_id, outcome, now_ms)?;
        candidate.check_size()?;
        *self = candidate;
        Ok(())
    }
    fn apply(&mut self, task_id: &str, outcome: Outcome, now_ms: u64) -> Result<(), EngineError> {
        if !matches!(self.status, RunStatus::Running | RunStatus::Paused) {
            return Err(error("Cannot complete a task on a terminal run"));
        }
        self.observe(now_ms)?;
        let task = self
            .nodes
            .values()
            .find_map(|s| match s {
                NodeState::Running { task } if task.id == task_id => Some(task.clone()),
                _ => None,
            })
            .ok_or_else(|| error("Task is not in flight (duplicate or stale completion)"))?;
        if now_ms >= self.deadline() {
            self.terminate(FailureCode::DeadlineExceeded);
            return Ok(());
        }
        let outcome = if now_ms >= task.deadline_ms {
            Outcome::Failed {
                failure: Failure::new(FailureCode::Timeout, true),
            }
        } else {
            outcome
        };
        let state = match outcome {
            Outcome::Complete { output, port } => {
                check_value(&output)?;
                if !Self::valid_port(&task.node, &port) {
                    return Err(error("Connector returned an invalid output port"));
                }
                NodeState::Succeeded {
                    output,
                    port,
                    attempts: task.attempt,
                }
            }
            Outcome::Wait { delay_ms, output } => {
                check_value(&output)?;
                let until_ms = now_ms
                    .checked_add(delay_ms)
                    .ok_or_else(|| error("Wait duration overflow"))?;
                if matches!(
                    task.node.kind.as_str(),
                    "IF / Condition" | "Switch / Router"
                ) {
                    return Err(error("Condition node cannot return a wait outcome"));
                }
                NodeState::Waiting {
                    until_ms,
                    output,
                    attempts: task.attempt,
                }
            }
            Outcome::Failed { failure } => {
                if failure.code == FailureCode::WorkflowFailed {
                    self.terminate(FailureCode::WorkflowFailed);
                    NodeState::Failed {
                        failure,
                        attempts: task.attempt,
                    }
                } else if failure.retryable && task.attempt < task.node.retry.max_attempts {
                    NodeState::Retry {
                        at_ms: now_ms.saturating_add(task.node.retry.backoff_ms(task.attempt)),
                        attempts: task.attempt,
                    }
                } else if self
                    .workflow
                    .edges
                    .iter()
                    .any(|e| e.source == task.node.id && e.source_port == "error")
                {
                    let mut output = json!({"error": {"code": failure.code}});
                    if let Some(detail) = failure.detail {
                        output["error"]["detail"] = json!(detail);
                    }
                    NodeState::Succeeded {
                        output,
                        port: "error".into(),
                        attempts: task.attempt,
                    }
                } else {
                    let collected = collected_failure(&self.workflow, &task.node.id);
                    if !collected {
                        self.status = RunStatus::Failed;
                        self.failure = Some(failure.code.clone());
                    }
                    NodeState::Failed {
                        failure,
                        attempts: task.attempt,
                    }
                }
            }
        };
        self.nodes.insert(task.node.id, state);
        Ok(())
    }
}
