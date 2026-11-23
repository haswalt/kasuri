//! Versioned workflow graph contracts, compatible with the React editor's JSON exports.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub mod contract;
pub use contract::{Diagnostic, json_schema, shape_diagnostics};
use serde_json::{Map, Value};

pub const SCHEMA_VERSION: u32 = 1;
pub const LATEST_SCHEMA_VERSION: u32 = 2;
pub const SUPPORTED_SCHEMA_VERSIONS: &[u32] = &[1, 2];
/// Connector kinds that start a run and therefore have no input port.
pub const TRIGGER_KINDS: &[&str] = &[
    "Manual Trigger",
    "Webhook",
    "Schedule",
    "Execute Workflow Trigger",
];
pub fn is_trigger_kind(kind: &str) -> bool {
    TRIGGER_KINDS.contains(&kind)
}
pub const MAX_NODES: usize = 100;
pub const MAX_GRAPH_BYTES: usize = 1_048_576;
pub const MAX_CONFIG_BYTES: usize = 65_536;
fn one() -> u32 {
    1
}
fn timeout() -> u64 {
    30_000
}
fn delay() -> u64 {
    1_000
}
fn max_delay() -> u64 {
    60_000
}
fn success() -> String {
    "success".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    #[schemars(range(min = 1, max = 2))]
    pub schema_version: u32,
    #[schemars(length(min = 1, max = 160))]
    pub id: String,
    #[schemars(length(min = 1, max = 160))]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub revision: u32,
    #[schemars(length(min = 1, max = 100))]
    pub nodes: Vec<Node>,
    #[schemars(length(max = 500))]
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub config: Map<String, Value>,
    /// Explicit typed assembly; omitted/empty retains legacy single-JSON input.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub input_bindings: Map<String, Value>,
    #[serde(default)]
    pub retry: RetryPolicy,
    #[serde(default = "timeout")]
    #[schemars(range(min = 1))]
    pub timeout_ms: u64,
}

impl Node {
    pub fn is_trigger(&self) -> bool {
        is_trigger_kind(&self.kind)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(default = "success")]
    pub source_port: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RetryPolicy {
    #[serde(default = "one")]
    #[schemars(range(min = 1, max = 10))]
    pub max_attempts: u32,
    #[serde(default = "delay")]
    pub delay_ms: u64,
    #[serde(default = "max_delay")]
    pub max_delay_ms: u64,
}
impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: one(),
            delay_ms: delay(),
            max_delay_ms: max_delay(),
        }
    }
}
impl RetryPolicy {
    pub fn backoff_ms(&self, failed_attempt: u32) -> u64 {
        self.delay_ms
            .saturating_mul(
                1u64.checked_shl(failed_attempt.saturating_sub(1))
                    .unwrap_or(u64::MAX),
            )
            .min(self.max_delay_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError(pub String);
impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for ValidationError {}

impl Workflow {
    pub fn from_json(json: &str) -> Result<Self, ValidationError> {
        if json.len() > MAX_GRAPH_BYTES {
            return Err(ValidationError("Workflow exceeds graph size limit".into()));
        }
        let workflow: Self = serde_json::from_str(json)
            .map_err(|_| ValidationError("Invalid workflow JSON".into()))?;
        workflow.validate()?;
        Ok(workflow)
    }

    /// Require one trigger, a connected directed acyclic graph and explicit branch ports.
    /// Node configuration and supported connector versions are checked by the adapter.
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self.diagnostics().first() {
            Some(d) => Err(ValidationError(d.message.clone())),
            None => Ok(()),
        }
    }
}
