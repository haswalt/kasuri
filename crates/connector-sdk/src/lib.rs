//! Runtime-independent task/result boundary. Hosts own HTTP, secrets and persistence.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use workflow_schema::Node;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    /// Stable across retries and recovery, scoped to run + node (not globally unique side effects).
    pub idempotency_key: String,
    pub node: Node,
    pub input: Value,
    /// Bounded, approved expression references, persisted with the task.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub context: Value,
    pub attempt: u32,
    pub deadline_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCode {
    InvalidConfig,
    MissingInput,
    UnsupportedNode,
    External,
    InvalidResponse,
    DuplicateResponse,
    EgressDenied,
    OutcomeUnknown,
    Timeout,
    BudgetExceeded,
    DeadlineExceeded,
    InvocationFailed,
}

/// Codes only: raw upstream error bodies may contain secrets and do not belong in checkpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub code: FailureCode,
    pub retryable: bool,
}
impl Failure {
    pub fn permanent(code: FailureCode) -> Self {
        Self {
            code,
            retryable: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Complete {
        output: Value,
        port: String,
    },
    /// Host schedules a wake-up; the engine never blocks or sleeps a thread.
    Wait {
        delay_ms: u64,
        output: Value,
    },
    Failed {
        failure: Failure,
    },
}
impl Outcome {
    pub fn success(output: Value) -> Self {
        Self::Complete {
            output,
            port: "success".into(),
        }
    }
}

/// A pure synchronous adapter interface; network hosts can dispatch Tasks asynchronously
/// and later submit Outcomes instead. No async runtime is required by the engine.
pub trait Connector {
    fn execute(&self, task: &Task) -> Outcome;
}

/// Versioned configuration metadata for discovery and future form generation.
/// Hosts must still validate schemas; metadata is not a permission grant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeManifest {
    pub kind: String,
    pub version: u32,
    pub config_schema: Value,
    pub output_ports: Vec<String>,
    pub has_external_side_effects: bool,
}
