//! Explicit single-branch routing and deterministic collection merging.
use connector_sdk::{Failure, FailureCode, Outcome};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
fn bad<T>() -> Result<T, FailureCode> {
    Err(FailureCode::InvalidConfig)
}
pub fn validate_switch(config: &Map<String, Value>) -> Result<(), FailureCode> {
    if config.len() != 1 {
        return bad();
    }
    let rules = config
        .get("rules")
        .and_then(Value::as_array)
        .filter(|r| !r.is_empty() && r.len() <= 8)
        .ok_or(FailureCode::InvalidConfig)?;
    let mut ports = BTreeSet::new();
    for r in rules {
        let row = r.as_object().ok_or(FailureCode::InvalidConfig)?;
        if row.len() != 2 {
            return bad();
        }
        let port = row
            .get("port")
            .and_then(Value::as_str)
            .ok_or(FailureCode::InvalidConfig)?;
        if port.is_empty()
            || port.len() > 32
            || !port
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
            || matches!(port, "fallback" | "error")
            || !ports.insert(port)
        {
            return bad();
        }
        super::conditions::validate(
            row.get("condition")
                .and_then(Value::as_object)
                .ok_or(FailureCode::InvalidConfig)?,
        )?;
    }
    Ok(())
}
pub fn switch(config: &Map<String, Value>, input: &Value) -> Outcome {
    let result = (|| {
        validate_switch(config)?;
        for rule in config["rules"].as_array().unwrap() {
            if super::conditions::evaluate(rule["condition"].as_object().unwrap(), input)? {
                return Ok(rule["port"].as_str().unwrap().to_owned());
            }
        }
        Ok("fallback".into())
    })();
    match result {
        Ok(port) => Outcome::Complete {
            output: input.clone(),
            port,
        },
        Err(code) => Outcome::Failed {
            failure: Failure::permanent(code),
        },
    }
}
pub fn validate_merge(config: &Map<String, Value>) -> Result<(), FailureCode> {
    if config.is_empty() {
        return Ok(());
    }
    let op = config
        .get("operation")
        .and_then(Value::as_str)
        .ok_or(FailureCode::InvalidConfig)?;
    if !matches!(op, "append" | "by_key" | "select") {
        return bad();
    }
    if config.keys().any(|k| {
        !matches!(
            k.as_str(),
            "operation" | "sources" | "arrayPointer" | "keyPointer" | "missing" | "conflict"
        )
    }) {
        return bad();
    }
    let sources = config
        .get("sources")
        .and_then(Value::as_array)
        .filter(|s| !s.is_empty() && s.len() <= 16)
        .ok_or(FailureCode::InvalidConfig)?;
    let mut unique = BTreeSet::new();
    for source in sources {
        let id = source
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or(FailureCode::InvalidConfig)?;
        if !unique.insert(id) {
            return bad();
        }
    }
    if op == "select"
        && (sources.len() != 1
            || config
                .keys()
                .any(|k| !matches!(k.as_str(), "operation" | "sources")))
    {
        return bad();
    }
    for key in ["arrayPointer", "keyPointer"] {
        if let Some(v) = config.get(key) {
            super::expressions::path(v)?;
        }
    }
    if config
        .get("missing")
        .is_some_and(|v| !matches!(v.as_str(), Some("fail" | "skip")))
        || config
            .get("conflict")
            .is_some_and(|v| !matches!(v.as_str(), Some("fail" | "last")))
    {
        return bad();
    }
    if op == "by_key"
        && !config
            .get("keyPointer")
            .and_then(Value::as_str)
            .is_some_and(|p| !p.is_empty())
    {
        return bad();
    }
    Ok(())
}
fn merge_value(config: &Map<String, Value>, input: &Value) -> Result<Value, FailureCode> {
    validate_merge(config)?;
    if config.is_empty() {
        return Ok(input.clone());
    }
    let op = config["operation"].as_str().unwrap();
    let sources = config["sources"].as_array().unwrap();
    let values = input.as_object().ok_or(FailureCode::InvalidConfig)?;
    if op == "select" {
        return values
            .get(sources[0].as_str().unwrap())
            .cloned()
            .ok_or(FailureCode::MissingInput);
    }
    let mut out: Vec<Value> = Vec::new();
    let mut keys = std::collections::BTreeMap::<String, usize>::new();
    let mut visited = 0;
    for source in sources {
        let Some(value) = values.get(source.as_str().unwrap()) else {
            if config.get("missing").and_then(Value::as_str) == Some("skip") {
                continue;
            }
            return Err(FailureCode::MissingInput);
        };
        let array = value
            .pointer(
                config
                    .get("arrayPointer")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            )
            .and_then(Value::as_array)
            .ok_or(FailureCode::InvalidConfig)?;
        for item in array {
            visited += 1;
            if visited > 1000 {
                return Err(FailureCode::BudgetExceeded);
            }
            if op == "append" {
                out.push(item.clone());
                continue;
            }
            let row = item.as_object().ok_or(FailureCode::InvalidConfig)?;
            let key = item
                .pointer(config["keyPointer"].as_str().unwrap())
                .filter(|v| v.is_string() || v.is_number() || v.is_boolean())
                .ok_or(FailureCode::MissingInput)?;
            let key = key.to_string();
            if let Some(index) = keys.get(&key) {
                let target = out[*index].as_object_mut().unwrap();
                for (field, value) in row {
                    if target.get(field).is_some_and(|old| old != value)
                        && config.get("conflict").and_then(Value::as_str) != Some("last")
                    {
                        return bad();
                    }
                    target.insert(field.clone(), value.clone());
                }
            } else {
                keys.insert(key, out.len());
                out.push(item.clone());
            }
        }
    }
    let result = Value::Array(out);
    if serde_json::to_vec(&result).unwrap().len() > 65_536 {
        return Err(FailureCode::BudgetExceeded);
    }
    Ok(result)
}
pub fn merge(config: &Map<String, Value>, input: &Value) -> Outcome {
    match merge_value(config, input) {
        Ok(output) => Outcome::success(output),
        Err(code) => Outcome::Failed {
            failure: Failure::permanent(code),
        },
    }
}
pub fn switch_schema() -> Value {
    json!({"type":"object","properties":{"rules":{"type":"array","minItems":1,"maxItems":8,"items":{"type":"object","properties":{"port":{"type":"string","pattern":"^[a-z0-9_-]{1,32}$"},"condition":super::conditions::schema()},"required":["port","condition"],"additionalProperties":false}}},"required":["rules"],"additionalProperties":false})
}
pub fn merge_schema() -> Value {
    json!({"type":"object","properties":{"operation":{"enum":["append","by_key","select"]},"sources":{"type":"array","items":{"type":"string"}},"arrayPointer":{"type":"string"},"keyPointer":{"type":"string"},"missing":{"enum":["fail","skip"]},"conflict":{"enum":["fail","last"]}},"additionalProperties":false})
}
