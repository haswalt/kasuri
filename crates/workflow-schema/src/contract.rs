//! Public contract generated from the same Serde types used by the engine.
use super::Workflow;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}
impl Diagnostic {
    pub fn new(code: &str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            path: path.into(),
            message: message.into(),
            expected: None,
            node_id: None,
            connector: None,
            suggestion: None,
        }
    }
    pub fn expected(mut self, value: Value) -> Self {
        self.expected = Some(value);
        self
    }
    pub fn node(mut self, id: &str, kind: &str) -> Self {
        self.node_id = Some(id.into());
        self.connector = Some(kind.into());
        self
    }
}
pub fn escape(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}
pub fn json_schema() -> Value {
    let mut schema =
        serde_json::to_value(schemars::schema_for!(Workflow)).expect("Rust graph schema");
    schema["$id"] = json!("urn:kasuri:workflow-definition:v2");
    schema["description"] = json!(
        "Kasuri workflow graph formats 1 and 2. Format 2 enables typed inputBindings. Node connector kind is the 'type' field; edges reference stable node IDs with source, target and sourcePort (default success). One trigger and an acyclic connected graph are required at execution/publication. Export metadata such as status/updatedAt is accepted separately and is server-managed on MCP writes. Use workflows_validate for semantic and connector diagnostics."
    );
    schema
}
/// Structural diagnostics intentionally contain no raw user values or deserializer messages.
pub fn shape_diagnostics(value: &Value) -> Vec<Diagnostic> {
    static VALIDATOR: std::sync::OnceLock<jsonschema::Validator> = std::sync::OnceLock::new();
    let validator = VALIDATOR.get_or_init(|| {
        jsonschema::validator_for(&json_schema()).expect("Generated workflow schema")
    });
    diagnostics(validator, value, "")
}
pub fn diagnostics(
    validator: &jsonschema::Validator,
    value: &Value,
    prefix: &str,
) -> Vec<Diagnostic> {
    use jsonschema::error::ValidationErrorKind;
    validator
        .iter_errors(value)
        .take(32)
        .map(|error| {
            let mut path = format!("{prefix}{}", error.instance_path());
            let (code, message, expected) = match error.kind() {
                ValidationErrorKind::Required { property } => {
                    if let Some(key) = property.as_str() {
                        path.push('/');
                        path.push_str(&escape(key));
                    }
                    ("REQUIRED_PROPERTY", "Required property is missing", None)
                }
                ValidationErrorKind::Type { kind } => (
                    "INVALID_TYPE",
                    "Property has the wrong JSON type",
                    Some(match kind {
                        jsonschema::error::TypeKind::Single(t) => json!(t.to_string()),
                        jsonschema::error::TypeKind::Multiple(types) => json!(format!("{types:?}")),
                    }),
                ),
                ValidationErrorKind::Enum { options } => (
                    "INVALID_ENUM",
                    "Choose a supported value",
                    Some((**options).clone()),
                ),
                ValidationErrorKind::Constant { expected_value } => (
                    "INVALID_VALUE",
                    "Property must match the supported constant",
                    Some((**expected_value).clone()),
                ),
                ValidationErrorKind::AdditionalProperties { .. } => (
                    "UNKNOWN_PROPERTY",
                    "Object contains unsupported properties",
                    None,
                ),
                _ => (
                    "INVALID_VALUE",
                    "Property does not satisfy its schema constraints",
                    None,
                ),
            };
            let mut diagnostic = Diagnostic::new(code, path, message);
            diagnostic.expected = expected;
            diagnostic
        })
        .collect()
}

impl Workflow {
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        use std::collections::{BTreeMap, BTreeSet, VecDeque};
        let mut out = Vec::new();
        if !super::SUPPORTED_SCHEMA_VERSIONS.contains(&self.schema_version) {
            out.push(
                Diagnostic::new(
                    "UNSUPPORTED_SCHEMA_VERSION",
                    "/schemaVersion",
                    "Unsupported workflow schema version",
                )
                .expected(json!(super::SUPPORTED_SCHEMA_VERSIONS)),
            );
        }
        for (path, value) in [("/id", &self.id), ("/name", &self.name)] {
            if value.trim().is_empty() {
                out.push(Diagnostic::new(
                    "REQUIRED_VALUE",
                    path,
                    "Use a non-empty value",
                ));
            }
        }
        if self.nodes.is_empty() || self.nodes.len() > super::MAX_NODES {
            out.push(Diagnostic::new(
                "NODE_COUNT",
                "/nodes",
                "Workflow must contain between 1 and 100 nodes",
            ));
        }
        if serde_json::to_vec(self).map_or(true, |v| v.len() > super::MAX_GRAPH_BYTES) {
            out.push(Diagnostic::new(
                "GRAPH_SIZE_LIMIT",
                "",
                "Workflow exceeds graph size limit",
            ));
        }
        if self.schema_version < 2 && self.nodes.iter().any(|n| !n.input_bindings.is_empty()) {
            out.push(
                Diagnostic::new(
                    "FEATURE_REQUIRES_SCHEMA_VERSION",
                    "/schemaVersion",
                    "Typed input bindings require workflow format 2",
                )
                .expected(json!(2)),
            );
        }
        let mut nodes = BTreeMap::new();
        for (i, n) in self.nodes.iter().enumerate() {
            for (key, value) in [("id", &n.id), ("type", &n.kind), ("label", &n.label)] {
                if value.trim().is_empty() {
                    out.push(
                        Diagnostic::new(
                            "REQUIRED_VALUE",
                            format!("/nodes/{i}/{key}"),
                            "Nodes require an ID, type and label",
                        )
                        .node(&n.id, &n.kind),
                    );
                }
            }
            if nodes.insert(n.id.as_str(), n).is_some() {
                out.push(
                    Diagnostic::new(
                        "DUPLICATE_NODE_ID",
                        format!("/nodes/{i}/id"),
                        "Node IDs must be unique",
                    )
                    .node(&n.id, &n.kind),
                );
            }
            if n.version == 0 {
                out.push(
                    Diagnostic::new(
                        "INVALID_CONNECTOR_VERSION",
                        format!("/nodes/{i}/version"),
                        "Connector version must be positive",
                    )
                    .node(&n.id, &n.kind),
                );
            }
            if n.timeout_ms == 0 {
                out.push(
                    Diagnostic::new(
                        "INVALID_TIMEOUT",
                        format!("/nodes/{i}/timeoutMs"),
                        "Node timeout must be positive",
                    )
                    .node(&n.id, &n.kind),
                );
            }
            if !(1..=10).contains(&n.retry.max_attempts) {
                out.push(
                    Diagnostic::new(
                        "INVALID_RETRY_POLICY",
                        format!("/nodes/{i}/retry/maxAttempts"),
                        "Choose between 1 and 10 attempts",
                    )
                    .node(&n.id, &n.kind),
                );
            }
            if n.retry.delay_ms > n.retry.max_delay_ms {
                out.push(
                    Diagnostic::new(
                        "INVALID_RETRY_POLICY",
                        format!("/nodes/{i}/retry/delayMs"),
                        "Retry delay must not exceed maxDelayMs",
                    )
                    .node(&n.id, &n.kind),
                );
            }
            if n.position
                .as_ref()
                .is_some_and(|p| !p.x.is_finite() || !p.y.is_finite())
            {
                out.push(
                    Diagnostic::new(
                        "INVALID_POSITION",
                        format!("/nodes/{i}/position"),
                        "Position coordinates must be finite",
                    )
                    .node(&n.id, &n.kind),
                );
            }
            if serde_json::to_vec(&n.config).map_or(true, |v| v.len() > super::MAX_CONFIG_BYTES) {
                out.push(
                    Diagnostic::new(
                        "CONFIG_SIZE_LIMIT",
                        format!("/nodes/{i}/config"),
                        "Node configuration exceeds size limit",
                    )
                    .node(&n.id, &n.kind),
                );
            }
            if out.len() >= 32 {
                return out.into_iter().take(32).collect();
            }
        }
        let triggers: Vec<_> = self.nodes.iter().filter(|n| n.is_trigger()).collect();
        if triggers.len() != 1 {
            out.push(
                Diagnostic::new("TRIGGER_COUNT", "/nodes", "Exactly one trigger is required")
                    .expected(json!([
                        "Manual Trigger",
                        "Webhook",
                        "Schedule",
                        "Execute Workflow Trigger"
                    ])),
            );
        }
        let mut ids = BTreeSet::new();
        let mut connections = BTreeSet::new();
        let mut indegree: BTreeMap<&str, usize> = nodes.keys().map(|id| (*id, 0)).collect();
        for (i, e) in self.edges.iter().enumerate() {
            if e.id.trim().is_empty() || !ids.insert(&e.id) {
                out.push(Diagnostic::new(
                    "DUPLICATE_EDGE_ID",
                    format!("/edges/{i}/id"),
                    "Edge IDs must be non-empty and unique",
                ));
            }
            if !connections.insert((&e.source, &e.target, &e.source_port)) {
                out.push(Diagnostic::new(
                    "DUPLICATE_CONNECTION",
                    format!("/edges/{i}"),
                    "Duplicate connection",
                ));
            }
            if let Some(source) = nodes.get(e.source.as_str()) {
                let ports = output_ports(source);
                if !ports.contains(&e.source_port) {
                    out.push(
                        Diagnostic::new(
                            "INVALID_SOURCE_PORT",
                            format!("/edges/{i}/sourcePort"),
                            "Choose an output port exposed by the source node",
                        )
                        .expected(json!(ports))
                        .node(&source.id, &source.kind),
                    );
                }
            } else {
                out.push(Diagnostic::new(
                    "UNKNOWN_SOURCE_NODE",
                    format!("/edges/{i}/source"),
                    "Edge source does not exist",
                ));
            }
            if let Some(count) = indegree.get_mut(e.target.as_str()) {
                *count += 1;
            } else {
                out.push(Diagnostic::new(
                    "UNKNOWN_TARGET_NODE",
                    format!("/edges/{i}/target"),
                    "Edge target does not exist",
                ));
            }
            if out.len() >= 32 {
                return out.into_iter().take(32).collect();
            }
        }
        for (i, n) in self.nodes.iter().enumerate() {
            if let Some(count) = indegree.get(n.id.as_str()) {
                if n.is_trigger() && *count != 0 {
                    out.push(
                        Diagnostic::new(
                            "TRIGGER_HAS_INPUT",
                            format!("/nodes/{i}"),
                            "Trigger cannot have incoming edges",
                        )
                        .node(&n.id, &n.kind),
                    );
                }
                if !n.is_trigger() && *count == 0 {
                    out.push(
                        Diagnostic::new(
                            "UNREACHABLE_NODE",
                            format!("/nodes/{i}"),
                            "Connect the node to an upstream output",
                        )
                        .node(&n.id, &n.kind),
                    );
                }
            }
        }
        if out.is_empty() {
            let mut queue: VecDeque<_> = indegree
                .iter()
                .filter(|(_, n)| **n == 0)
                .map(|(id, _)| *id)
                .collect();
            let mut visited = 0;
            while let Some(id) = queue.pop_front() {
                visited += 1;
                for edge in self.edges.iter().filter(|e| e.source == id) {
                    if let Some(n) = indegree.get_mut(edge.target.as_str()) {
                        *n -= 1;
                        if *n == 0 {
                            queue.push_back(&edge.target);
                        }
                    }
                }
            }
            if visited != self.nodes.len() {
                out.push(Diagnostic::new(
                    "CYCLIC_GRAPH",
                    "/edges",
                    "Graph contains a cycle or unreachable nodes",
                ));
            }
        }
        out.into_iter().take(32).collect()
    }
}
pub fn output_ports(node: &super::Node) -> Vec<String> {
    if node.kind == "Fail Workflow" {
        vec![]
    } else if node.kind == "IF / Condition" {
        vec!["true".into(), "false".into(), "error".into()]
    } else if node.kind == "Switch / Router" {
        node.config
            .get("rules")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|r| r["port"].as_str().map(str::to_owned))
            .chain(["fallback".into(), "error".into()])
            .collect()
    } else {
        vec!["success".into(), "error".into()]
    }
}
