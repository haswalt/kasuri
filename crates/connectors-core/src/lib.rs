//! Bounded declarative nodes. No eval, subprocesses, secrets or network access.
use connector_sdk::{Connector, Failure, FailureCode, Outcome, Task};
use serde_json::{Map, Value};

pub mod conditions;
pub use connectors_validation as validation;
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
            "Validate Schema" => validation::execute(task),
            "IF / Condition" => match conditions::evaluate(&node.config, &task.input) {
                Ok(passed) => Outcome::Complete {
                    output: task.input.clone(),
                    port: if passed { "true" } else { "false" }.into(),
                },
                Err(code) => Outcome::Failed {
                    failure: Failure::permanent(code),
                },
            },
            "Filter" => {
                let Some(items) = task
                    .input
                    .pointer(
                        node.config
                            .get("arrayPointer")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                    )
                    .and_then(Value::as_array)
                else {
                    return invalid();
                };
                match conditions::filter(&node.config, items) {
                    Ok(items) => Outcome::success(Value::Array(items)),
                    Err(code) => Outcome::Failed {
                        failure: Failure::permanent(code),
                    },
                }
            }
            "Sort" => {
                let Some(items) = task
                    .input
                    .pointer(
                        node.config
                            .get("arrayPointer")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                    )
                    .and_then(Value::as_array)
                else {
                    return invalid();
                };
                let pointer = node
                    .config
                    .get("pointer")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let order = node
                    .config
                    .get("order")
                    .and_then(Value::as_str)
                    .unwrap_or("asc");
                if !matches!(order, "asc" | "desc") {
                    return invalid();
                }
                let mut items = items.clone();
                // Reject missing keys and mixed types instead of silently changing ordering.
                let numeric = items
                    .first()
                    .and_then(|item| item.pointer(pointer))
                    .is_some_and(Value::is_number);
                if items.iter().any(|item| {
                    !item.pointer(pointer).is_some_and(|key| {
                        if numeric {
                            key.is_number()
                        } else {
                            key.is_string()
                        }
                    })
                }) {
                    return invalid();
                }
                items.sort_by(|a, b| {
                    let a = a.pointer(pointer).unwrap();
                    let b = b.pointer(pointer).unwrap();
                    let result = if numeric {
                        compare_numbers(a, b)
                    } else {
                        a.as_str().unwrap().cmp(b.as_str().unwrap())
                    };
                    if order == "desc" {
                        result.reverse()
                    } else {
                        result
                    }
                });
                Outcome::success(Value::Array(items))
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
        ("Webhook", json!({"type":"object","properties":{"authentication":{"enum":["none","bearer","query","hmac_sha256","hmac"]},"signingSecret":{"type":"string"},"queryParameter":{"type":"string","default":"key"},"contextHeaders":{"type":"array","items":{"type":"string"}},"contextQuery":{"type":"array","items":{"type":"string"}}}})),
        ("Schedule", json!({"type":"object","properties":{"cron":{"type":"string"}}})),
        ("Filter", { let mut schema = conditions::schema(); schema["properties"]["arrayPointer"] = json!({"type":"string"}); schema }),
        ("Sort", json!({"type":"object","properties":{"arrayPointer":{"type":"string"},"pointer":{"type":"string"},"order":{"enum":["asc","desc"]}}})),
        ("Merge", json!({"type":"object","properties":{}})),
        ("Error Handler", json!({"type":"object","properties":{}})),
        ("Map / Transform", json!({"type":"object","properties":{"mapping":{"oneOf":[{"type":"object"},{"type":"string","description":"JSON-encoded mapping object"}]}},"required":["mapping"]})),
        ("IF / Condition", conditions::schema()),
        ("Wait / Delay", json!({"type":"object","properties":{"delayMs":{"oneOf":[{"type":"integer","minimum":0},{"type":"string","pattern":"^[0-9]+$"}]}},"required":["delayMs"]})),
        ("JSON Utilities", json!({"type":"object","properties":{"operation":{"enum":["parse","stringify"]}},"required":["operation"]})),
    ].into_iter().map(|(kind, config_schema)| connector_sdk::NodeManifest {
        kind: kind.into(), version: 1, config_schema,
        output_ports: if kind == "IF / Condition" { vec!["true".into(),"false".into(),"error".into()] } else { vec!["success".into(),"error".into()] },
        has_external_side_effects: false,
    }).chain(std::iter::once(validation::manifest())).collect()
}

fn compare_numbers(a: &Value, b: &Value) -> std::cmp::Ordering {
    if let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) {
        return a.cmp(&b);
    }
    if let (Some(a), Some(b)) = (a.as_u64(), b.as_u64()) {
        return a.cmp(&b);
    }
    if a.as_i64().is_some_and(|n| n < 0) && b.as_u64().is_some() {
        return std::cmp::Ordering::Less;
    }
    if b.as_i64().is_some_and(|n| n < 0) && a.as_u64().is_some() {
        return std::cmp::Ordering::Greater;
    }
    a.as_f64()
        .unwrap()
        .partial_cmp(&b.as_f64().unwrap())
        .unwrap()
}
