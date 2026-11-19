//! Offline, bounded JSON Schema validation. Branching and HTTP policy stay in
//! normal workflow nodes; invalid data is a successful result with valid=false.
use connector_sdk::{Failure, FailureCode, NodeManifest, Outcome, Task};
use jsonschema::{Draft, Validator, error::ValidationErrorKind};
use serde_json::{Map, Value, json};
fn invalid<T>() -> Result<T, FailureCode> {
    Err(FailureCode::InvalidConfig)
}
fn guard(schema: &Value, depth: usize, nodes: &mut usize) -> Result<(), FailureCode> {
    *nodes += 1;
    if depth > 16 || *nodes > 512 {
        return invalid();
    }
    if schema.is_boolean() {
        return Ok(());
    }
    let object = schema.as_object().ok_or(FailureCode::InvalidConfig)?;
    for (key, value) in object {
        match key.as_str() {
            "properties" => {
                for child in value
                    .as_object()
                    .ok_or(FailureCode::InvalidConfig)?
                    .values()
                {
                    guard(child, depth + 1, nodes)?;
                }
            }
            "items" | "additionalProperties" => guard(value, depth + 1, nodes)?,
            "$schema" if value.as_str() == Some("https://json-schema.org/draft/2020-12/schema") => {
            }
            "type" | "required" | "minItems" | "maxItems" | "minLength" | "maxLength"
            | "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" | "multipleOf"
            | "enum" | "const" | "minProperties" | "maxProperties" | "title" | "description"
            | "default" => {}
            "format"
                if matches!(
                    value.as_str(),
                    Some(
                        "date"
                            | "date-time"
                            | "time"
                            | "email"
                            | "hostname"
                            | "ipv4"
                            | "ipv6"
                            | "uri"
                            | "uuid"
                    )
                ) => {}
            _ => return invalid(),
        }
    }
    Ok(())
}
pub fn validate(config: &Map<String, Value>) -> Result<Validator, FailureCode> {
    if config
        .keys()
        .any(|k| !matches!(k.as_str(), "schema" | "pointer"))
        || config.get("pointer").is_some_and(|v| !v.is_string())
    {
        return invalid();
    }
    let schema = config.get("schema").ok_or(FailureCode::InvalidConfig)?;
    let schema = match schema {
        Value::String(text) => {
            serde_json::from_str(text).map_err(|_| FailureCode::InvalidConfig)?
        }
        value => value.clone(),
    };
    if schema.to_string().len() > 65536 {
        return Err(FailureCode::BudgetExceeded);
    }
    guard(&schema, 0, &mut 0)?;
    jsonschema::options()
        .offline()
        .with_draft(Draft::Draft202012)
        .should_validate_formats(true)
        .should_ignore_unknown_formats(false)
        .build(&schema)
        .map_err(|_| FailureCode::InvalidConfig)
}
fn clipped(text: &str) -> String {
    text.chars().take(256).collect()
}
fn result(task: &Task) -> Result<Value, FailureCode> {
    let validator = validate(&task.node.config)?;
    let pointer = task
        .node
        .config
        .get("pointer")
        .and_then(Value::as_str)
        .unwrap_or("");
    let value = task
        .input
        .pointer(pointer)
        .ok_or(FailureCode::MissingInput)?;
    let mut iterator = validator.iter_errors(value).peekable();
    let valid = iterator.peek().is_none();
    let mut errors = Vec::new();
    let mut remaining = 8192;
    let mut truncated = false;
    for error in iterator {
        if errors.len() >= 32 {
            truncated = true;
            break;
        }
        let instance = error.instance_path().to_string();
        let schema = error.schema_path().to_string();
        let keyword = error.kind().keyword();
        let mut detail = json!({"instancePath":clipped(&instance),"schemaPath":clipped(&schema),"keyword":keyword,"message":format!("Value does not satisfy {keyword}"),"pathTruncated":instance.chars().count()>256||schema.chars().count()>256});
        if let ValidationErrorKind::Required { property } = error.kind() {
            detail["missingProperty"] = json!(clipped(property.as_str().unwrap_or("")));
        }
        let size = detail.to_string().len();
        if size > remaining {
            truncated = true;
            break;
        }
        remaining -= size;
        errors.push(detail);
    }
    let output = json!({"valid":valid,"errors":errors,"errorsTruncated":truncated,"value":value});
    if output.to_string().len() > 65536 {
        return Err(FailureCode::BudgetExceeded);
    }
    Ok(output)
}
pub fn execute(task: &Task) -> Outcome {
    if task.node.version != 1 {
        return Outcome::Failed {
            failure: Failure::permanent(FailureCode::InvalidConfig),
        };
    }
    match result(task) {
        Ok(output) => Outcome::success(output),
        Err(code) => Outcome::Failed {
            failure: Failure::permanent(code),
        },
    }
}
pub fn manifest() -> NodeManifest {
    NodeManifest {
        kind: "Validate Schema".into(),
        version: 1,
        config_schema: json!({"type":"object","properties":{"schema":{"type":"object","description":"Offline bounded JSON Schema 2020-12 profile; types, required fields, properties/items, ranges, enums and supported formats."},"pointer":{"type":"string","description":"Optional JSON Pointer; empty selects the whole input."}},"required":["schema"],"additionalProperties":false}),
        output_ports: vec!["success".into(), "error".into()],
        has_external_side_effects: false,
    }
}
