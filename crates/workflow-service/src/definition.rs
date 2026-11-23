//! Discovery and diagnostics shared by MCP and platform API; no workspace mutation.
use crate::{Document, Publication, Workspace, manifests};
use serde_json::{Value, json};
use std::sync::OnceLock;
use workflow_schema::{Diagnostic, Workflow};
pub fn schema() -> Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(||{
  let mut s=workflow_schema::json_schema();
  let kinds:Vec<_>=manifests().into_iter().map(|m|m.kind).collect();
  s["$defs"]["Node"]["properties"]["type"]["enum"]=json!(kinds);
  let rules:Vec<Value>=manifests().into_iter().enumerate().map(|(index,m)|{
   let mut config=m.config_schema;config["$id"]=json!(format!("urn:kasuri:connector:{index}:{}",m.version));
   let needs_config=!jsonschema::validator_for(&config).expect("Trusted connector schema").is_valid(&json!({}));
   let mut rule=json!({"if":{"properties":{"type":{"const":m.kind}},"required":["type"]},"then":{"properties":{"version":{"const":m.version},"config":config}}});
   if needs_config {rule["then"]["required"]=json!(["config"]);}
   rule
  }).collect();
  s["$defs"]["Node"]["allOf"]=json!(rules);
  s["$defs"]["Node"]["properties"]["inputBindings"]=connectors_core::expressions::bindings_schema();
  s["properties"]["schemaVersion"]["enum"]=json!(workflow_schema::SUPPORTED_SCHEMA_VERSIONS);
  s["allOf"]=json!([{"if":{"properties":{"nodes":{"contains":{"type":"object","properties":{"inputBindings":{"type":"object","minProperties":1}},"required":["inputBindings"]}}},"required":["nodes"]},"then":{"properties":{"schemaVersion":{"const":2}}}}]);
  s
 }).clone()
}
pub fn discovery() -> Value {
    json!({"schemaVersion":workflow_schema::LATEST_SCHEMA_VERSION,"supportedSchemaVersions":workflow_schema::SUPPORTED_SCHEMA_VERSIONS,"schema":schema(),"validationTool":"workflows_validate","examplesTool":"workflows_examples_list","notes":["Node connector kind uses type, not connectorKind; label is required.","Default version=1, sourcePort=success, timeoutMs=30000 and retry.maxAttempts=1.","Graph schema/config checks are followed by topology and runtime-specific connector checks; always validate before creating.","Export metadata is accepted; MCP resets status/updatedAt and controls edit version. Publication is separate.","Switch output ports include configured rule names plus fallback/error. IF uses true/false/error; other nodes use success/error."]})
}
fn full_validator() -> &'static jsonschema::Validator {
    static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| {
        jsonschema::validator_for(&schema())
            .expect("Workflow definition schema must compile offline")
    })
}
pub fn schema_errors(value: &Value) -> Vec<Diagnostic> {
    workflow_schema::contract::diagnostics(full_validator(), value, "")
}
pub fn shape_errors(value: &Value) -> Vec<Diagnostic> {
    workflow_schema::shape_diagnostics(value).into_iter().map(|mut error| {
        if let Some(index)=error.path.strip_prefix("/nodes/").and_then(|p|p.split('/').next()).and_then(|p|p.parse::<usize>().ok()) {
            if let Some(node)=value.get("nodes").and_then(Value::as_array).and_then(|nodes|nodes.get(index)) {
                error.node_id=node.get("id").and_then(Value::as_str).map(str::to_owned);
                error.connector=node.get("type").and_then(Value::as_str).map(str::to_owned);
                if error.path.ends_with("/type") {error.suggestion=Some("Use the type property with an implemented name from connectors_list; connectorKind is not a graph field".into());}
            }
        }
        error
    }).collect()
}
pub fn errors(value: &Value) -> Vec<Diagnostic> {
    if value.to_string().len() > crate::MAX_BODY_BYTES {
        return vec![Diagnostic::new(
            "DOCUMENT_SIZE_LIMIT",
            "",
            "Workflow definition exceeds the 128 KiB management request limit",
        )];
    }
    let mut errors = shape_errors(value);
    if !errors.is_empty() {
        return errors;
    }
    let graph: Workflow = match serde_json::from_value(value.clone()) {
        Ok(g) => g,
        Err(_) => {
            return vec![Diagnostic::new(
                "INVALID_WORKFLOW_DOCUMENT",
                "",
                "Use the fields and types in workflows_schema_get",
            )];
        }
    };
    errors.extend(graph.diagnostics());
    let schema_errors = workflow_schema::contract::diagnostics(full_validator(), value, "");
    for mut error in schema_errors {
        if let Some(index) = error
            .path
            .strip_prefix("/nodes/")
            .and_then(|s| s.split('/').next())
            .and_then(|s| s.parse::<usize>().ok())
        {
            if let Some(node) = graph.nodes.get(index) {
                error.node_id = Some(node.id.clone());
                error.connector = Some(node.kind.clone());
                if !manifests().iter().any(|m| m.kind == node.kind) {
                    error.code = "UNSUPPORTED_CONNECTOR".into();
                    error.suggestion =
                        Some("Choose an implemented type from connectors_list".into());
                }
            }
        }
        if !errors
            .iter()
            .any(|d| d.path == error.path && d.code == error.code)
        {
            errors.push(error);
        }
    }
    if errors.is_empty() {
        let doc = Document {
            graph,
            status: Publication::Draft,
            updated_at: String::new(),
            imported_from: None,
            lifecycle: None,
            has_unpublished_changes: false,
            archived_at: None,
            version: 0,
            next_scheduled_at: None,
        };
        if let Err(failure) = Workspace::executable(&doc) {
            if failure.diagnostics.is_empty() {
                errors.push(Diagnostic::new(
                    "INVALID_EXECUTION_CONFIG",
                    "/nodes",
                    failure.message,
                ));
            } else {
                errors.extend(failure.diagnostics);
            }
        }
    }
    errors.into_iter().take(32).collect()
}
pub fn validate(value: &Value) -> Value {
    let errors = errors(value);
    json!({"valid":errors.is_empty(),"schemaVersion":value.get("schemaVersion").cloned().unwrap_or(json!(workflow_schema::LATEST_SCHEMA_VERSION)),"errors":errors,"errorsTruncated":errors.len()==32})
}
