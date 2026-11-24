//! Bounded item orchestration over durable, pinned child workflows.
use crate::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub index: usize,
    pub item_id: String,
    pub status: Status,
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<FailureCode>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Iteration {
    pub items: Vec<Item>,
}
pub(crate) fn validate(node: &workflow_schema::Node) -> Result<(), ApiError> {
    let config = &node.config;
    if config.keys().any(|k| {
        !matches!(
            k.as_str(),
            "workflowId" | "revision" | "arrayPointer" | "concurrency" | "failurePolicy"
        )
    }) || config
        .get("workflowId")
        .and_then(Value::as_str)
        .is_none_or(|s| s.is_empty() || s.len() > 160)
        || config
            .get("revision")
            .is_some_and(|v| v.as_u64().is_none_or(|n| n == 0 || n > u32::MAX as u64))
        || config.get("arrayPointer").is_some_and(|v| {
            !v.is_string()
                || (!v.as_str().unwrap().is_empty() && !v.as_str().unwrap().starts_with('/'))
        })
        || config.get("concurrency").is_some_and(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                .is_none_or(|n| !matches!(n, 1 | 2 | 4))
        })
        || config
            .get("failurePolicy")
            .is_some_and(|v| !matches!(v.as_str(), Some("fail" | "collect")))
        || node.retry.max_attempts != 1
    {
        return Err(error(
            400,
            "Each requires a target, array pointer, concurrency 1/2/4 and fail/collect policy. Configure retries on the child nodes.",
        ));
    }
    Ok(())
}
impl Workspace {
    pub(crate) fn settle_items(&mut self, now: u64) -> Result<bool, ApiError> {
        let pending: Vec<(String, Task)> = self
            .runs
            .iter()
            .filter(|r| r.status().is_ok_and(|s| s == RunStatus::Running))
            .flat_map(|r| {
                let id = r.id_string().unwrap();
                r.external
                    .iter()
                    .chain(r.pending.iter())
                    .filter(|e| e.task.node.kind == "Each / Loop")
                    .map(move |e| (id.clone(), e.task.clone()))
            })
            .collect();
        let mut changed = false;
        for record in &mut self.runs {
            if record
                .status()
                .is_ok_and(|s| matches!(s, RunStatus::Failed | RunStatus::Cancelled))
            {
                for iteration in record.iterations.values_mut() {
                    for item in &mut iteration.items {
                        if matches!(item.status, Status::Pending | Status::Running) {
                            item.status = Status::Cancelled;
                            item.error = Some(FailureCode::InvocationFailed);
                            changed = true;
                        }
                    }
                }
            }
        }

        for (id, task) in pending {
            let index = self.run_index(&id).unwrap();
            if !self.runs[index]
                .status()
                .is_ok_and(|s| s == RunStatus::Running)
            {
                continue;
            }
            let time = if self.runs[index].mode == Mode::Simulation {
                self.runs[index].logical_clock
            } else {
                now
            };
            let array = task
                .input
                .pointer(
                    task.node
                        .config
                        .get("arrayPointer")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )
                .and_then(Value::as_array);
            let Some(array) = array else {
                self.finish_invocation(
                    &id,
                    &task,
                    connectors_http::failure(FailureCode::MissingInput, false),
                    time,
                )?;
                changed = true;
                continue;
            };
            if array.len() > 100 {
                self.finish_invocation(
                    &id,
                    &task,
                    connectors_http::failure(FailureCode::BudgetExceeded, false),
                    time,
                )?;
                changed = true;
                continue;
            }
            let mut state = self.runs[index]
                .iterations
                .get(&task.node.id)
                .cloned()
                .unwrap_or_else(|| Iteration {
                    items: (0..array.len())
                        .map(|index| Item {
                            index,
                            item_id: format!(
                                "item-{:x}",
                                Sha256::digest(
                                    json!([task.idempotency_key, "item", index])
                                        .to_string()
                                        .as_bytes()
                                )
                            ),
                            status: Status::Pending,
                            run_id: None,
                            output: None,
                            error: None,
                        })
                        .collect(),
                });
            if state.items.len() != array.len()
                || state.items.iter().enumerate().any(|(index, item)| {
                    item.index != index
                        || item.item_id
                            != format!(
                                "item-{:x}",
                                Sha256::digest(
                                    json!([task.idempotency_key, "item", index])
                                        .to_string()
                                        .as_bytes()
                                )
                            )
                        || (item.status == Status::Running && item.run_id.is_none())
                })
            {
                return Err(error(500, "Invalid stored item state"));
            }
            let mut clock = time;
            let mut output_bytes = state
                .items
                .iter()
                .filter_map(|i| i.output.as_ref())
                .map(|v| v.to_string().len())
                .sum::<usize>();
            for item in &mut state.items {
                if item.status != Status::Running {
                    continue;
                }
                let child = item.run_id.as_deref().and_then(|id| self.run_index(id));
                let status = child
                    .map(|i| self.runs[i].status())
                    .transpose()?
                    .unwrap_or(RunStatus::Failed);
                if matches!(status, RunStatus::Running | RunStatus::Paused) {
                    continue;
                }
                if let Some(index) = child {
                    if self.runs[index].mode == Mode::Simulation {
                        clock = clock.max(self.runs[index].logical_clock)
                    }
                }
                match child
                    .filter(|_| status == RunStatus::Succeeded)
                    .map(|i| Self::child_output(&self.runs[i]))
                {
                    Some(Ok(output)) => {
                        if output_bytes.saturating_add(output.to_string().len()) > 32768 {
                            item.status = Status::Failed;
                            item.error = Some(FailureCode::BudgetExceeded)
                        } else {
                            output_bytes += output.to_string().len();
                            item.status = Status::Succeeded;
                            item.output = Some(output)
                        }
                    }
                    _ => {
                        item.status = if status == RunStatus::Cancelled {
                            Status::Cancelled
                        } else {
                            Status::Failed
                        };
                        item.error = Some(FailureCode::InvocationFailed)
                    }
                }
                let parent = self.run_index(&id).unwrap();
                if let Some(link) = self.runs[parent]
                    .children
                    .iter_mut()
                    .find(|c| Some(&c.run_id) == item.run_id.as_ref())
                {
                    link.collected = true;
                    link.status = status;
                }
                changed = true;
            }
            let failed = state
                .items
                .iter()
                .any(|i| matches!(i.status, Status::Failed | Status::Cancelled));
            let oversized = serde_json::to_vec(&state)
                .map_err(|e| error(500, e.to_string()))?
                .len()
                > 60_000;
            let expired = clock >= task.deadline_ms;
            if oversized
                || expired
                || (failed
                    && task
                        .node
                        .config
                        .get("failurePolicy")
                        .and_then(Value::as_str)
                        != Some("collect"))
            {
                for item in &mut state.items {
                    if matches!(item.status, Status::Pending | Status::Running) {
                        if let Some(id) = &item.run_id {
                            self.cancel_child(id)?;
                        }
                        item.status = Status::Cancelled;
                        item.error = Some(FailureCode::InvocationFailed)
                    }
                    if oversized {
                        item.output = None;
                        if item.status == Status::Succeeded {
                            item.status = Status::Failed;
                            item.error = Some(FailureCode::BudgetExceeded)
                        }
                    }
                }
                let parent = self.run_index(&id).unwrap();
                self.runs[parent]
                    .iterations
                    .insert(task.node.id.clone(), state);
                self.finish_invocation(
                    &id,
                    &task,
                    connectors_http::failure(
                        if oversized {
                            FailureCode::BudgetExceeded
                        } else if expired {
                            FailureCode::Timeout
                        } else {
                            FailureCode::InvocationFailed
                        },
                        false,
                    ),
                    clock,
                )?;
                changed = true;
                continue;
            }
            let limit = task
                .node
                .config
                .get("concurrency")
                .and_then(|v| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                })
                .unwrap_or(1) as usize;
            // Reserve progress for nested child invocations within the workspace's
            // active-run budget instead of filling every slot with waiting parents.
            let key = crate::invocation::target_key(&task.node);
            let nested = self.runs[self.run_index(&id).unwrap()]
                .invocation_targets
                .get(&key)
                .is_some_and(|g| {
                    g.nodes
                        .iter()
                        .any(|n| matches!(n.kind.as_str(), "Execute Workflow" | "Each / Loop"))
                });
            let limit = if nested { 1 } else { limit };
            let mut active = state
                .items
                .iter()
                .filter(|i| i.status == Status::Running)
                .count();
            for item in &mut state.items {
                if item.status != Status::Pending || active >= limit {
                    continue;
                }
                if self
                    .runs
                    .iter()
                    .filter(|r| {
                        r.status()
                            .is_ok_and(|s| matches!(s, RunStatus::Running | RunStatus::Paused))
                    })
                    .count()
                    >= 5
                {
                    break;
                }
                let mut item_task = task.clone();
                item_task.idempotency_key =
                    json!([task.idempotency_key, "item", item.index]).to_string();
                item_task.input = array[item.index].clone();
                item_task.node.kind = "Execute Workflow".into();
                item_task
                    .node
                    .config
                    .retain(|k, _| matches!(k.as_str(), "workflowId" | "revision"));
                item_task.node.config.insert("mode".into(), json!("await"));
                let parent = self.run_index(&id).unwrap();
                match self.create_item_child(parent, &item_task, clock, Some(item.index)) {
                    Ok(child) => {
                        item.run_id = Some(child.run_id);
                        item.status = Status::Running;
                        active += 1
                    }
                    Err(_) => {
                        item.status = Status::Failed;
                        item.error = Some(FailureCode::InvocationFailed)
                    }
                }
                changed = true;
                if item.status == Status::Failed
                    && task
                        .node
                        .config
                        .get("failurePolicy")
                        .and_then(Value::as_str)
                        != Some("collect")
                {
                    break;
                }
            }
            let parent = self.run_index(&id).unwrap();
            self.runs[parent].logical_clock = clock;
            self.runs[parent]
                .iterations
                .insert(task.node.id.clone(), state.clone());
            let may_complete = task
                .node
                .config
                .get("failurePolicy")
                .and_then(Value::as_str)
                == Some("collect")
                || !state
                    .items
                    .iter()
                    .any(|i| matches!(i.status, Status::Failed | Status::Cancelled));
            if may_complete
                && state.items.iter().all(|i| {
                    matches!(
                        i.status,
                        Status::Succeeded | Status::Failed | Status::Cancelled
                    )
                })
            {
                let succeeded = state
                    .items
                    .iter()
                    .filter(|i| i.status == Status::Succeeded)
                    .count();
                let failed = state.items.len() - succeeded;
                self.finish_invocation(&id,&task,Outcome::success(json!({"items":state.items,"total":array.len(),"succeeded":succeeded,"failed":failed})),clock)?;
                changed = true;
            } else {
                let wake = state
                    .items
                    .iter()
                    .filter_map(|item| {
                        item.run_id
                            .as_ref()
                            .and_then(|id| self.run_index(id))
                            .and_then(|i| self.runs[i].wake_at)
                    })
                    .min()
                    .unwrap_or(now.saturating_add(100))
                    .min(task.deadline_ms);
                let next = wake.max(now.saturating_add(1));
                self.runs[parent].wake_at = Some(if self.runs[parent].pending.is_empty() {
                    next
                } else {
                    self.runs[parent].wake_at.unwrap_or(u64::MAX).min(next)
                });
            }
        }
        if changed {
            self.checkpoint()?;
        }
        Ok(changed)
    }
}
