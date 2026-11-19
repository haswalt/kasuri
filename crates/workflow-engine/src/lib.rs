//! Deterministic bounded DAG scheduler. A host persists each checkpoint before dispatching
//! its task, executes connectors, then persists the reported outcome before advancing.
//! This crate does not itself provide durable storage or exactly-once side effects.
use connector_sdk::{Failure, FailureCode, Outcome, Task};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use workflow_schema::Workflow;

pub const CHECKPOINT_VERSION: u32 = 1;
const MAX_CHECKPOINT_BYTES: usize = 2_097_152;
const MAX_VALUE_BYTES: usize = 65_536;

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
    if serde_json::to_vec(value)
        .map_err(|_| error("Invalid JSON value"))?
        .len()
        > MAX_VALUE_BYTES
    {
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
        run.checkpoint()?;
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

    pub fn checkpoint(&self) -> Result<String, EngineError> {
        let text = serde_json::to_string(self).map_err(|_| error("Cannot serialize checkpoint"))?;
        if text.len() > MAX_CHECKPOINT_BYTES {
            return Err(error("Checkpoint exceeds 2 MiB; reduce output sizes"));
        }
        Ok(text)
    }

    /// Restore only host-owned checkpoints, never arbitrary user-provided run state.
    pub fn restore(text: &str) -> Result<Self, EngineError> {
        if text.len() > MAX_CHECKPOINT_BYTES {
            return Err(error("Checkpoint exceeds size limit"));
        }
        let run: Self = serde_json::from_str(text).map_err(|_| error("Invalid checkpoint JSON"))?;
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
                > 1
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
                    if !Self::valid_port(&node.kind, port) {
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
            && !run
                .nodes
                .values()
                .all(|s| matches!(s, NodeState::Succeeded { .. } | NodeState::Skipped))
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
    fn valid_port(kind: &str, port: &str) -> bool {
        if kind == "IF / Condition" {
            matches!(port, "true" | "false" | "error")
        } else {
            matches!(port, "success" | "error")
        }
    }

    /// Select at most one task. Repeated calls while it is in flight return the same task.
    /// All joins wait for every parent to settle; their input is keyed by active parent ID.
    pub fn next(&mut self, now_ms: u64) -> Result<Dispatch, EngineError> {
        // Apply each state transition atomically, including checkpoint budget enforcement.
        let mut candidate = self.clone();
        let dispatch = candidate.advance(now_ms)?;
        candidate.checkpoint()?;
        *self = candidate;
        Ok(dispatch)
    }
    fn advance(&mut self, now_ms: u64) -> Result<Dispatch, EngineError> {
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
        if let Some(task) = self.nodes.values().find_map(|s| {
            if let NodeState::Running { task } = s {
                Some(task.clone())
            } else {
                None
            }
        }) {
            if now_ms < task.deadline_ms {
                return Ok(Dispatch::Task(Box::new(task)));
            }
            self.apply(
                &task.id,
                Outcome::Failed {
                    failure: Failure {
                        code: FailureCode::Timeout,
                        retryable: true,
                    },
                },
                now_ms,
            )?;
            if self.status == RunStatus::Failed {
                return Ok(Dispatch::Finished(self.status));
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
                if !parents.is_empty() && active.is_empty() {
                    self.nodes.insert(node.id.clone(), NodeState::Skipped);
                    skipped = true;
                    continue;
                }
                if self.dispatches >= self.limits.max_dispatches {
                    return Ok(self.terminate(FailureCode::BudgetExceeded));
                }
                let input = if parents.is_empty() {
                    self.input.clone()
                } else if parents.len() == 1 {
                    active[0].1.clone()
                } else {
                    Value::Object(active.into_iter().collect())
                };
                check_value(&input)?;
                let task = Task {
                    id: self.task_id(&node.id, attempt),
                    idempotency_key: self.effect_key(&node.id),
                    node: node.clone(),
                    input,
                    attempt,
                    deadline_ms: now_ms.saturating_add(node.timeout_ms).min(self.deadline()),
                };
                self.dispatches += 1;
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
                NodeState::Retry { at_ms, .. } => Some(*at_ms),
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
        candidate.checkpoint()?;
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
                failure: Failure {
                    code: FailureCode::Timeout,
                    retryable: true,
                },
            }
        } else {
            outcome
        };
        let state = match outcome {
            Outcome::Complete { output, port } => {
                check_value(&output)?;
                if !Self::valid_port(&task.node.kind, &port) {
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
                if task.node.kind == "IF / Condition" {
                    return Err(error("Condition node cannot return a wait outcome"));
                }
                NodeState::Waiting {
                    until_ms,
                    output,
                    attempts: task.attempt,
                }
            }
            Outcome::Failed { failure } => {
                if failure.retryable && task.attempt < task.node.retry.max_attempts {
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
                    NodeState::Succeeded {
                        output: json!({"error": {"code": failure.code}}),
                        port: "error".into(),
                        attempts: task.attempt,
                    }
                } else {
                    self.status = RunStatus::Failed;
                    self.failure = Some(failure.code.clone());
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
