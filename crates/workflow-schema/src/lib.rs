//! Versioned workflow graph contracts, compatible with the React editor's JSON exports.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const SCHEMA_VERSION: u32 = 1;
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub revision: u32,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub retry: RetryPolicy,
    #[serde(default = "timeout")]
    pub timeout_ms: u64,
}

impl Node {
    pub fn is_trigger(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "Manual Trigger" | "Webhook" | "Schedule" | "Execute Workflow Trigger"
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(default = "success")]
    pub source_port: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryPolicy {
    #[serde(default = "one")]
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
        let fail = |message: &str| Err(ValidationError(message.into()));
        if self.schema_version != SCHEMA_VERSION {
            return fail("Unsupported workflow schema version");
        }
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return fail("Workflow ID and name are required");
        }
        if self.nodes.is_empty() || self.nodes.len() > MAX_NODES {
            return fail("Workflow must contain between 1 and 100 nodes");
        }
        if serde_json::to_vec(self)
            .map_err(|_| ValidationError("Cannot serialize graph".into()))?
            .len()
            > MAX_GRAPH_BYTES
        {
            return fail("Workflow exceeds graph size limit");
        }
        let mut nodes = BTreeMap::new();
        for node in &self.nodes {
            if node
                .position
                .as_ref()
                .is_some_and(|p| !p.x.is_finite() || !p.y.is_finite())
            {
                return fail("Invalid node position");
            }
            if node.id.trim().is_empty()
                || node.kind.trim().is_empty()
                || node.label.trim().is_empty()
                || node.version == 0
            {
                return fail("Nodes require an ID, type, label and positive version");
            }
            if nodes.insert(node.id.as_str(), node).is_some() {
                return fail("Duplicate node ID");
            }
            if node.timeout_ms == 0 {
                return fail("Node timeout must be positive");
            }
            if !(1..=10).contains(&node.retry.max_attempts)
                || node.retry.delay_ms > node.retry.max_delay_ms
            {
                return fail("Invalid retry policy (1–10 attempts, delay <= max delay)");
            }
            if serde_json::to_vec(&node.config).unwrap().len() > MAX_CONFIG_BYTES {
                return fail("Node configuration exceeds size limit");
            }
        }
        let triggers: Vec<_> = self.nodes.iter().filter(|n| n.is_trigger()).collect();
        if triggers.len() != 1 {
            return fail("Exactly one trigger is required");
        }
        let mut edge_ids = BTreeSet::new();
        let mut connections = BTreeSet::new();
        let mut indegree: BTreeMap<&str, usize> = nodes.keys().map(|id| (*id, 0)).collect();
        for edge in &self.edges {
            if edge.id.trim().is_empty() || !edge_ids.insert(&edge.id) {
                return fail("Empty or duplicate edge ID");
            }
            let Some(source) = nodes.get(edge.source.as_str()) else {
                return fail("Edge source does not exist");
            };
            if !nodes.contains_key(edge.target.as_str()) {
                return fail("Edge target does not exist");
            }
            if !connections.insert((&edge.source, &edge.target, &edge.source_port)) {
                return fail("Duplicate connection");
            }
            let valid_port = if source.kind == "IF / Condition" {
                matches!(edge.source_port.as_str(), "true" | "false" | "error")
            } else {
                matches!(edge.source_port.as_str(), "success" | "error")
            };
            if !valid_port {
                return fail("Unsupported source port");
            }
            *indegree.get_mut(edge.target.as_str()).unwrap() += 1;
        }
        if indegree[triggers[0].id.as_str()] != 0 {
            return fail("Trigger cannot have incoming edges");
        }
        if self
            .nodes
            .iter()
            .any(|n| !n.is_trigger() && indegree[n.id.as_str()] == 0)
        {
            return fail("Unreachable node");
        }
        let mut queue = VecDeque::from([triggers[0].id.as_str()]);
        let mut visited = 0;
        while let Some(id) = queue.pop_front() {
            visited += 1;
            for edge in self.edges.iter().filter(|e| e.source == id) {
                let count = indegree.get_mut(edge.target.as_str()).unwrap();
                *count -= 1;
                if *count == 0 {
                    queue.push_back(&edge.target);
                }
            }
        }
        if visited != self.nodes.len() {
            return fail("Graph contains a cycle or unreachable nodes");
        }
        Ok(())
    }
}
