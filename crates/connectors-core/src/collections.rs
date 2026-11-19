//! Explicit collection adapters: arrays never imply item execution on their own.
use connector_sdk::{Failure, FailureCode, Outcome};
use serde_json::{Map, Value, json};
fn pointer(config: &Map<String, Value>, key: &str) -> Result<String, FailureCode> {
    match config.get(key) {
        None => Ok(String::new()),
        Some(Value::String(s)) if s.is_empty() || s.starts_with('/') => Ok(s.clone()),
        _ => Err(FailureCode::InvalidConfig),
    }
}
pub fn validate(kind: &str, config: &Map<String, Value>) -> Result<(), FailureCode> {
    pointer(config, "arrayPointer")?;
    if kind == "Split / Batch" {
        if config
            .keys()
            .any(|k| !matches!(k.as_str(), "arrayPointer" | "batchSize"))
            || config
                .get("batchSize")
                .is_some_and(|v| number(v).is_none_or(|n| !(1..=100).contains(&n)))
        {
            return Err(FailureCode::InvalidConfig);
        }
    } else {
        pointer(config, "pointer")?;
        if config.keys().any(|k| {
            !matches!(
                k.as_str(),
                "arrayPointer" | "pointer" | "operation" | "missing"
            )
        }) || !matches!(
            config.get("operation").and_then(Value::as_str),
            Some("collect" | "count" | "sum")
        ) || config
            .get("missing")
            .is_some_and(|v| !matches!(v.as_str(), Some("fail" | "skip")))
        {
            return Err(FailureCode::InvalidConfig);
        }
    }
    Ok(())
}
fn number(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}
fn evaluate(kind: &str, config: &Map<String, Value>, input: &Value) -> Result<Value, FailureCode> {
    validate(kind, config)?;
    let array = input
        .pointer(&pointer(config, "arrayPointer")?)
        .and_then(Value::as_array)
        .ok_or(FailureCode::MissingInput)?;
    if array.len() > 1000 {
        return Err(FailureCode::BudgetExceeded);
    }
    if kind == "Split / Batch" {
        let size = config.get("batchSize").and_then(number).unwrap_or(10) as usize;
        return Ok(
            json!({"batches":array.chunks(size).collect::<Vec<_>>(),"total":array.len(),"batchSize":size}),
        );
    }
    if config["operation"] == "count" {
        return Ok(json!(array.len()));
    }
    let path = pointer(config, "pointer")?;
    let mut values = Vec::new();
    for item in array {
        match item.pointer(&path) {
            Some(value) => values.push(value.clone()),
            None if config.get("missing").and_then(Value::as_str) == Some("skip") => {}
            None => return Err(FailureCode::MissingInput),
        }
    }
    if config["operation"] == "collect" {
        return Ok(Value::Array(values));
    }
    let mut integer = 0i128;
    let mut float = 0f64;
    let mut all_integer = true;
    for value in &values {
        let n = value.as_number().ok_or(FailureCode::InvalidResponse)?;
        let exact = n
            .as_i64()
            .map(i128::from)
            .or_else(|| n.as_u64().map(i128::from));
        if let Some(n) = exact {
            integer += n
        } else {
            all_integer = false
        }
        float += n.as_f64().ok_or(FailureCode::InvalidResponse)?;
    }
    if all_integer {
        if let Ok(value) = i64::try_from(integer) {
            Ok(json!(value))
        } else if let Ok(value) = u64::try_from(integer) {
            Ok(json!(value))
        } else {
            Err(FailureCode::BudgetExceeded)
        }
    } else {
        serde_json::Number::from_f64(float)
            .map(Value::Number)
            .ok_or(FailureCode::BudgetExceeded)
    }
}
pub fn execute(kind: &str, config: &Map<String, Value>, input: &Value) -> Outcome {
    match evaluate(kind, config, input) {
        Ok(value) if value.to_string().len() <= 65536 => Outcome::success(value),
        Ok(_) => Outcome::Failed {
            failure: Failure::permanent(FailureCode::BudgetExceeded),
        },
        Err(code) => Outcome::Failed {
            failure: Failure::permanent(code),
        },
    }
}
