//! Bounded JSON expression trees. No source-code evaluation or host access.
use connector_sdk::{Failure, FailureCode, Outcome, Task};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
const MAX_NODES: usize = 256;
const MAX_DEPTH: usize = 12;
const MAX_BYTES: usize = 65_536;
fn bad<T>() -> Result<T, FailureCode> {
    Err(FailureCode::InvalidConfig)
}
fn obj(v: &Value) -> Result<&Map<String, Value>, FailureCode> {
    v.as_object().ok_or(FailureCode::InvalidConfig)
}
pub fn path(v: &Value) -> Result<&str, FailureCode> {
    let p = v.as_str().ok_or(FailureCode::InvalidConfig)?;
    if p.len() > 256 || (!p.is_empty() && !p.starts_with('/')) {
        return bad();
    }
    let mut chars = p.chars();
    while let Some(c) = chars.next() {
        if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return bad();
        }
    }
    Ok(p)
}
fn arity(op: &str) -> Option<(usize, usize)> {
    Some(match op {
        "add" | "subtract" | "multiply" | "divide" | "equals" | "greater_than" | "less_than"
        | "date_add_ms" => (2, 2),
        "if" => (3, 3),
        "concat" | "coalesce" | "and" | "or" => (1, 16),
        "lower" | "upper" | "trim" | "length" | "exists" | "not" | "date_parse" | "date_iso"
        | "to_text" => (1, 1),
        _ => return None,
    })
}
fn inspect(
    v: &Value,
    depth: usize,
    count: &mut usize,
    refs: &mut BTreeSet<String>,
) -> Result<(), FailureCode> {
    *count += 1;
    if depth > MAX_DEPTH || *count > MAX_NODES {
        return Err(FailureCode::BudgetExceeded);
    }
    let o = obj(v)?;
    let op = o
        .get("op")
        .and_then(Value::as_str)
        .ok_or(FailureCode::InvalidConfig)?;
    let keys: &[&str] = match op {
        "literal" => &["op", "value"],
        "input" | "trigger" | "run" | "workflow" => &["op", "path"],
        "node" => &["op", "nodeId", "path"],
        _ => &["op", "args"],
    };
    if o.keys().any(|k| !keys.contains(&k.as_str())) {
        return bad();
    }
    match op {
        "literal" => {
            if !o.contains_key("value") {
                return bad();
            }
        }
        "input" | "trigger" | "run" | "workflow" | "node" => {
            path(o.get("path").ok_or(FailureCode::InvalidConfig)?)?;
            if op == "node" {
                let id = o
                    .get("nodeId")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty() && s.len() <= 160)
                    .ok_or(FailureCode::InvalidConfig)?;
                refs.insert(id.into());
            }
        }
        _ => {
            let (min, max) = arity(op).ok_or(FailureCode::InvalidConfig)?;
            let args = o
                .get("args")
                .and_then(Value::as_array)
                .ok_or(FailureCode::InvalidConfig)?;
            if args.len() < min || args.len() > max {
                return bad();
            }
            for a in args {
                inspect(a, depth + 1, count, refs)?;
            }
        }
    }
    Ok(())
}
pub fn validate(config: &Map<String, Value>) -> Result<BTreeSet<String>, FailureCode> {
    let fields = config
        .get("fields")
        .and_then(Value::as_object)
        .filter(|_| config.len() == 1);
    let Some(fields) = fields else {
        // Oversized configs report a budget failure whatever their shape.
        if serde_json::to_vec(config).map_or(true, |v| v.len() > MAX_BYTES) {
            return Err(FailureCode::BudgetExceeded);
        }
        return bad();
    };
    validate_fields(fields)
}
/// Validate named expressions as if wrapped in an Expressions `{"fields":…}` config.
pub fn validate_fields(fields: &Map<String, Value>) -> Result<BTreeSet<String>, FailureCode> {
    // `{"fields":` plus the closing brace keeps the budget identical to validate().
    const WRAPPER_BYTES: usize = 11;
    if serde_json::to_vec(fields).map_or(true, |v| v.len() + WRAPPER_BYTES > MAX_BYTES) {
        return Err(FailureCode::BudgetExceeded);
    }
    if fields.is_empty() || fields.len() > 32 {
        return bad();
    }
    let mut count = 0;
    let mut refs = BTreeSet::new();
    for (key, v) in fields {
        if key.is_empty() || key.len() > 128 {
            return bad();
        }
        inspect(v, 0, &mut count, &mut refs)?
    }
    Ok(refs)
}
fn required(v: Option<Value>) -> Result<Value, FailureCode> {
    v.ok_or(FailureCode::MissingInput)
}
fn integer(v: &Value) -> Option<i128> {
    v.as_i64()
        .map(i128::from)
        .or_else(|| v.as_u64().map(i128::from))
}
fn int_value(n: i128) -> Result<Value, FailureCode> {
    if let Ok(n) = i64::try_from(n) {
        Ok(json!(n))
    } else if let Ok(n) = u64::try_from(n) {
        Ok(json!(n))
    } else {
        Err(FailureCode::BudgetExceeded)
    }
}
fn bounded(v: Value) -> Result<Value, FailureCode> {
    if serde_json::to_vec(&v)
        .map_err(|_| FailureCode::InvalidConfig)?
        .len()
        > MAX_BYTES
    {
        Err(FailureCode::BudgetExceeded)
    } else {
        Ok(v)
    }
}
fn eval(v: &Value, task: &Task) -> Result<Option<Value>, FailureCode> {
    let o = v.as_object().unwrap();
    let op = o["op"].as_str().unwrap();
    if op == "literal" {
        return Ok(Some(o["value"].clone()));
    }
    if matches!(op, "input" | "trigger" | "run" | "workflow" | "node") {
        let source = match op {
            "input" => Some(&task.input),
            "trigger" => task.context.get("trigger"),
            "run" => task.context.get("run"),
            "workflow" => task.context.get("workflow"),
            _ => task
                .context
                .get("nodes")
                .and_then(|n| n.get(o["nodeId"].as_str().unwrap())),
        };
        return Ok(source
            .and_then(|s| s.pointer(o["path"].as_str().unwrap()))
            .cloned());
    }
    let args = o["args"].as_array().unwrap();
    let get = |i: usize| eval(&args[i], task);
    let req = |i: usize| required(get(i)?);
    let result = match op {
        "exists" => json!(get(0)?.is_some()),
        "coalesce" => {
            let mut found = None;
            for a in args {
                let value = eval(a, task)?;
                if value.as_ref().is_some_and(|v| !v.is_null()) {
                    found = value;
                    break;
                }
            }
            return Ok(Some(found.unwrap_or(Value::Null)));
        }
        "if" => {
            let yes = req(0)?.as_bool().ok_or(FailureCode::InvalidConfig)?;
            return get(if yes { 1 } else { 2 });
        }
        "and" | "or" => {
            let mut answer = op == "and";
            for a in args {
                let value = required(eval(a, task)?)?
                    .as_bool()
                    .ok_or(FailureCode::InvalidConfig)?;
                answer = value;
                if (op == "and" && !value) || (op == "or" && value) {
                    break;
                }
            }
            json!(answer)
        }
        "not" => json!(!req(0)?.as_bool().ok_or(FailureCode::InvalidConfig)?),
        "equals" => {
            let a = req(0)?;
            let b = req(1)?;
            json!(if let (Some(a), Some(b)) = (a.as_number(), b.as_number()) {
                number_cmp(a, b).is_eq()
            } else {
                a == b
            })
        }
        "greater_than" | "less_than" => {
            let a = req(0)?;
            let b = req(1)?;
            let order = if let (Some(a), Some(b)) = (a.as_number(), b.as_number()) {
                number_cmp(a, b)
            } else if let (Some(a), Some(b)) = (a.as_str(), b.as_str()) {
                a.cmp(b)
            } else {
                return bad();
            };
            json!(if op == "greater_than" {
                order.is_gt()
            } else {
                order.is_lt()
            })
        }
        "add" | "subtract" | "multiply" | "divide" => {
            let a = req(0)?;
            let b = req(1)?;
            if op != "divide" && integer(&a).is_some() && integer(&b).is_some() {
                let (a, b) = (integer(&a).unwrap(), integer(&b).unwrap());
                int_value(
                    match op {
                        "add" => a.checked_add(b),
                        "subtract" => a.checked_sub(b),
                        _ => a.checked_mul(b),
                    }
                    .ok_or(FailureCode::BudgetExceeded)?,
                )?
            } else {
                let a = a.as_f64().ok_or(FailureCode::InvalidConfig)?;
                let b = b.as_f64().ok_or(FailureCode::InvalidConfig)?;
                if op == "divide" && b == 0.0 {
                    return bad();
                }
                let n = match op {
                    "add" => a + b,
                    "subtract" => a - b,
                    "multiply" => a * b,
                    _ => a / b,
                };
                Value::Number(serde_json::Number::from_f64(n).ok_or(FailureCode::BudgetExceeded)?)
            }
        }
        "concat" => {
            let mut text = String::new();
            for a in args {
                let v = required(eval(a, task)?)?;
                text.push_str(v.as_str().ok_or(FailureCode::InvalidConfig)?);
                if text.len() > MAX_BYTES {
                    return Err(FailureCode::BudgetExceeded);
                }
            }
            Value::String(text)
        }
        "to_text" => {
            let v = req(0)?;
            Value::String(if let Some(s) = v.as_str() {
                s.into()
            } else {
                v.to_string()
            })
        }
        "lower" | "upper" | "trim" => {
            let v = req(0)?;
            let s = v.as_str().ok_or(FailureCode::InvalidConfig)?;
            Value::String(match op {
                "lower" => s.to_lowercase(),
                "upper" => s.to_uppercase(),
                _ => s.trim().into(),
            })
        }
        "length" => {
            let v = req(0)?;
            json!(match &v {
                Value::String(s) => s.chars().count(),
                Value::Array(a) => a.len(),
                Value::Object(o) => o.len(),
                _ => return bad(),
            })
        }
        "date_parse" => {
            let v = req(0)?;
            let s = v.as_str().ok_or(FailureCode::InvalidConfig)?;
            json!(
                chrono::DateTime::parse_from_rfc3339(s)
                    .map_err(|_| FailureCode::InvalidConfig)?
                    .timestamp_millis()
            )
        }
        "date_iso" => {
            let n = req(0)?.as_i64().ok_or(FailureCode::InvalidConfig)?;
            json!(
                chrono::DateTime::from_timestamp_millis(n)
                    .ok_or(FailureCode::InvalidConfig)?
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            )
        }
        "date_add_ms" => {
            let a = req(0)?.as_i64().ok_or(FailureCode::InvalidConfig)?;
            let b = req(1)?.as_i64().ok_or(FailureCode::InvalidConfig)?;
            json!(a.checked_add(b).ok_or(FailureCode::BudgetExceeded)?)
        }
        _ => return bad(),
    };
    Ok(Some(bounded(result)?))
}
/// Assemble a node input using the same evaluator as the Expressions connector.
pub fn assemble(fields: &Map<String, Value>, task: &Task) -> Result<Value, FailureCode> {
    validate_fields(fields)?;
    evaluate(fields, task)
}
/// Evaluate fields that have already passed `validate_fields`; eval relies on that shape.
fn evaluate(fields: &Map<String, Value>, task: &Task) -> Result<Value, FailureCode> {
    let mut output = Map::new();
    for (name, expr) in fields {
        output.insert(name.clone(), required(eval(expr, task)?)?);
    }
    bounded(Value::Object(output))
}
pub fn execute(task: &Task) -> Outcome {
    let result = validate(&task.node.config)
        .and_then(|_| evaluate(task.node.config["fields"].as_object().unwrap(), task));
    match result {
        Ok(value) => Outcome::success(value),
        Err(code) => Outcome::Failed {
            failure: Failure::permanent(code),
        },
    }
}
pub fn bindings_schema() -> Value {
    let expressions = schema();
    let mut fields = expressions["properties"]["fields"].clone();
    fields["$id"] = json!("urn:kasuri:input-bindings:v1");
    fields["$defs"] = expressions["$defs"].clone();
    fields["minProperties"] = json!(0);
    fields["description"] = json!(
        "Optional typed assembly of a non-trigger node's input. Empty/omitted preserves normal input. References use original incoming input and graph-ancestor outputs. The assembled object is persisted before dispatch; config stays literal."
    );
    fields
}
pub fn schema() -> Value {
    let reference = json!({"$ref":"#/$defs/expression"});
    json!({
        "type":"object", "properties":{"fields":{"type":"object","minProperties":1,"maxProperties":32,"propertyNames":{"minLength":1,"maxLength":128},"additionalProperties":reference}},
        "required":["fields"], "additionalProperties":false,
        "$defs":{"expression":{"oneOf":[
            {"type":"object","properties":{"op":{"const":"literal"},"value":{}},"required":["op","value"],"additionalProperties":false},
            {"type":"object","properties":{"op":{"enum":["input","trigger","run","workflow"]},"path":{"type":"string","maxLength":256}},"required":["op","path"],"additionalProperties":false},
            {"type":"object","properties":{"op":{"const":"node"},"nodeId":{"type":"string","minLength":1,"maxLength":160},"path":{"type":"string","maxLength":256}},"required":["op","nodeId","path"],"additionalProperties":false},
            {"type":"object","properties":{"op":{"enum":["add","subtract","multiply","divide","equals","greater_than","less_than","date_add_ms","if","concat","coalesce","and","or","lower","upper","trim","length","exists","not","date_parse","date_iso","to_text"]},"args":{"type":"array","minItems":1,"maxItems":16,"items":reference}},"required":["op","args"],"additionalProperties":false}
        ]}},"description":"Bounded expression trees: at most 256 nodes and depth 12. Unary operations take 1 argument, arithmetic/comparisons/date_add_ms take 2, if takes 3; concat/coalesce/and/or take 1–16. Node references must be graph ancestors."
    })
}

pub fn number_cmp(a: &serde_json::Number, b: &serde_json::Number) -> std::cmp::Ordering {
    let integer = |n: &serde_json::Number| {
        n.as_i64()
            .map(i128::from)
            .or_else(|| n.as_u64().map(i128::from))
    };
    if let (Some(a), Some(b)) = (integer(a), integer(b)) {
        return a.cmp(&b);
    }
    a.as_f64()
        .unwrap()
        .partial_cmp(&b.as_f64().unwrap())
        .unwrap()
}
