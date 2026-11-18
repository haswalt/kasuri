//! Bounded declarative nodes. No eval, subprocesses, secrets or network access.
use connector_sdk::{Connector, Failure, FailureCode, Outcome, Task};
use serde_json::{Map, Value};

pub struct CoreConnector;

fn invalid() -> Outcome {
    Outcome::Failed {
        failure: Failure::permanent(FailureCode::InvalidConfig),
    }
}
fn config_json(value: &Value) -> Option<Value> {
    match value {
        Value::String(text) => serde_json::from_str(text).ok(),
        other => Some(other.clone()),
    }
}

impl Connector for CoreConnector {
    fn execute(&self, task: &Task) -> Outcome {
        let node = &task.node;
        if node.version != 1 {
            return invalid();
        }
        match node.kind.as_str() {
            // These consume an already authenticated/validated event from the host.
            "Manual Trigger" | "Webhook" | "Schedule" | "Merge" | "Error Handler" => {
                Outcome::success(task.input.clone())
            }
            "Map / Transform" => {
                let Some(Value::Object(mapping)) = node.config.get("mapping").and_then(config_json)
                else {
                    return invalid();
                };
                let mut output = Map::new();
                for (key, value) in mapping {
                    // Explicit JSON Pointer references; every other value is a JSON literal.
                    let value = if let Some(pointer) = value
                        .as_object()
                        .filter(|o| o.len() == 1)
                        .and_then(|o| o.get("$input"))
                        .and_then(Value::as_str)
                    {
                        let Some(value) = task.input.pointer(pointer) else {
                            return Outcome::Failed {
                                failure: Failure::permanent(FailureCode::MissingInput),
                            };
                        };
                        value.clone()
                    } else {
                        value
                    };
                    output.insert(key, value);
                }
                Outcome::success(Value::Object(output))
            }
            "IF / Condition" => {
                let Some(pointer) = node.config.get("pointer").and_then(Value::as_str) else {
                    return invalid();
                };
                let Some(expected) = node.config.get("equals") else {
                    return invalid();
                };
                let Some(actual) = task.input.pointer(pointer) else {
                    return Outcome::Failed {
                        failure: Failure::permanent(FailureCode::MissingInput),
                    };
                };
                Outcome::Complete {
                    output: task.input.clone(),
                    port: if actual == expected { "true" } else { "false" }.into(),
                }
            }
            "Wait / Delay" => {
                let delay = node
                    .config
                    .get("delayMs")
                    .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()));
                match delay {
                    Some(delay_ms) => Outcome::Wait {
                        delay_ms,
                        output: task.input.clone(),
                    },
                    None => invalid(),
                }
            }
            "JSON Utilities" => match node.config.get("operation").and_then(Value::as_str) {
                Some("parse") => match task
                    .input
                    .as_str()
                    .and_then(|text| serde_json::from_str(text).ok())
                {
                    Some(value) => Outcome::success(value),
                    None => invalid(),
                },
                Some("stringify") => Outcome::success(Value::String(task.input.to_string())),
                _ => invalid(),
            },
            _ => Outcome::Failed {
                failure: Failure::permanent(FailureCode::UnsupportedNode),
            },
        }
    }
}

/// Only implemented node types are advertised here, independently of the UI's planned catalogue.
pub fn manifests() -> Vec<connector_sdk::NodeManifest> {
    use serde_json::json;
    [
        ("Manual Trigger", json!({"type":"object","properties":{}})),
        ("Webhook", json!({"type":"object","properties":{"path":{"type":"string"}}})),
        ("Schedule", json!({"type":"object","properties":{"cron":{"type":"string"}}})),
        ("Merge", json!({"type":"object","properties":{}})),
        ("Error Handler", json!({"type":"object","properties":{}})),
        ("Map / Transform", json!({"type":"object","properties":{"mapping":{"oneOf":[{"type":"object"},{"type":"string","description":"JSON-encoded mapping object"}]}},"required":["mapping"]})),
        ("IF / Condition", json!({"type":"object","properties":{"pointer":{"type":"string","description":"JSON Pointer into the input"},"equals":{}},"required":["pointer","equals"]})),
        ("Wait / Delay", json!({"type":"object","properties":{"delayMs":{"oneOf":[{"type":"integer","minimum":0},{"type":"string","pattern":"^[0-9]+$"}]}},"required":["delayMs"]})),
        ("JSON Utilities", json!({"type":"object","properties":{"operation":{"enum":["parse","stringify"]}},"required":["operation"]})),
    ].into_iter().map(|(kind, config_schema)| connector_sdk::NodeManifest {
        kind: kind.into(), version: 1, config_schema,
        output_ports: if kind == "IF / Condition" { vec!["true".into(),"false".into(),"error".into()] } else { vec!["success".into(),"error".into()] },
        has_external_side_effects: false,
    }).collect()
}
