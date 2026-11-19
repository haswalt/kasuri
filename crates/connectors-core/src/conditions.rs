//! Structured conditions with explicit types and bounded work. Legacy equality
//! remains supported without changing its missing-input/error semantics.
use connector_sdk::FailureCode;
use serde_json::{Map, Value, json};
use std::cmp::Ordering;
#[derive(Clone)]
struct Rule {
    field: String,
    operator: String,
    value: Value,
}
fn bad<T>() -> Result<T, FailureCode> {
    Err(FailureCode::InvalidConfig)
}
fn pointer(field: &str) -> Result<String, FailureCode> {
    if field.is_empty() || field.len() > 256 {
        return bad();
    }
    if field == "$" {
        return Ok(String::new());
    }
    if field.starts_with('/') {
        let bytes = field.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'~' {
                index += 1;
                if index == bytes.len() || !matches!(bytes[index], b'0' | b'1') {
                    return bad();
                }
            }
            index += 1;
        }
        return Ok(field.into());
    }
    if field.split('.').any(str::is_empty) {
        return bad();
    }
    Ok(field
        .split('.')
        .map(|s| format!("/{}", s.replace('~', "~0").replace('/', "~1")))
        .collect())
}
fn expected(rule: &Map<String, Value>) -> Result<Value, FailureCode> {
    let value = rule.get("value").ok_or(FailureCode::InvalidConfig)?;
    let Some(kind) = rule.get("valueType") else {
        return Ok(value.clone());
    };
    let Some(kind) = kind.as_str() else {
        return bad();
    };
    let parsed = || {
        if let Value::String(text) = value {
            serde_json::from_str(text).map_err(|_| FailureCode::InvalidConfig)
        } else {
            Ok(value.clone())
        }
    };
    match kind {
        "string" if value.is_string() => Ok(value.clone()),
        "number" => parsed().and_then(|v| if v.is_number() { Ok(v) } else { bad() }),
        "boolean" => parsed().and_then(|v| if v.is_boolean() { Ok(v) } else { bad() }),
        "null" => Ok(Value::Null),
        "json" => parsed(),
        _ => bad(),
    }
}
fn rules(config: &Map<String, Value>) -> Result<(bool, Vec<Rule>), FailureCode> {
    if config
        .keys()
        .any(|k| !matches!(k.as_str(), "conditions" | "match"))
    {
        return bad();
    }
    let any = match config.get("match").and_then(Value::as_str).unwrap_or("all") {
        "all" => false,
        "any" => true,
        _ => return bad(),
    };
    if config.get("match").is_some_and(|v| !v.is_string()) {
        return bad();
    }
    let conditions =
        super::config_json(config.get("conditions").ok_or(FailureCode::InvalidConfig)?)
            .ok_or(FailureCode::InvalidConfig)?;
    let values = conditions
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 32)
        .ok_or(FailureCode::InvalidConfig)?;
    let mut rules = Vec::new();
    for value in values {
        let row = value.as_object().ok_or(FailureCode::InvalidConfig)?;
        if row
            .keys()
            .any(|k| !matches!(k.as_str(), "field" | "operator" | "value" | "valueType"))
        {
            return bad();
        }
        let field = pointer(
            row.get("field")
                .and_then(Value::as_str)
                .ok_or(FailureCode::InvalidConfig)?,
        )?;
        let operator = row
            .get("operator")
            .and_then(Value::as_str)
            .ok_or(FailureCode::InvalidConfig)?;
        let unary = matches!(
            operator,
            "exists" | "not_exists" | "is_empty" | "is_not_empty"
        );
        if !unary
            && !matches!(
                operator,
                "equals"
                    | "not_equals"
                    | "contains"
                    | "not_contains"
                    | "starts_with"
                    | "ends_with"
                    | "greater_than"
                    | "greater_or_equal"
                    | "less_than"
                    | "less_or_equal"
            )
        {
            return bad();
        }
        let value = if unary { Value::Null } else { expected(row)? };
        if matches!(
            operator,
            "greater_than" | "greater_or_equal" | "less_than" | "less_or_equal"
        ) && !value.is_number()
        {
            return bad();
        }
        if matches!(operator, "starts_with" | "ends_with") && !value.is_string() {
            return bad();
        }
        rules.push(Rule {
            field,
            operator: operator.into(),
            value,
        });
    }
    Ok((any, rules))
}
pub fn validate(config: &Map<String, Value>) -> Result<(), FailureCode> {
    if config.contains_key("conditions") {
        rules(config).map(|_| ())
    } else if config.get("pointer").is_some_and(Value::is_string) && config.contains_key("equals") {
        Ok(())
    } else {
        bad()
    }
}
pub fn number_cmp(a: &serde_json::Number, b: &serde_json::Number) -> Ordering {
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
fn equal(a: &Value, b: &Value) -> bool {
    if let (Value::Number(a), Value::Number(b)) = (a, b) {
        number_cmp(a, b) == Ordering::Equal
    } else {
        a == b
    }
}
fn empty(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Array(v)) => v.is_empty(),
        Some(Value::Object(v)) => v.is_empty(),
        _ => false,
    }
}
pub fn evaluate(config: &Map<String, Value>, input: &Value) -> Result<bool, FailureCode> {
    if !config.contains_key("conditions") {
        let field = config
            .get("pointer")
            .and_then(Value::as_str)
            .ok_or(FailureCode::InvalidConfig)?;
        let expected = config.get("equals").ok_or(FailureCode::InvalidConfig)?;
        return input
            .pointer(field)
            .map(|value| value == expected)
            .ok_or(FailureCode::MissingInput);
    }
    let (any, rules) = rules(config)?;
    Ok(evaluate_rules(any, &rules, input))
}
fn evaluate_rules(any: bool, rules: &[Rule], input: &Value) -> bool {
    let matches = |rule: &Rule| {
        let actual = input.pointer(&rule.field);
        match rule.operator.as_str() {
            "exists" => return actual.is_some(),
            "not_exists" => return actual.is_none(),
            "is_empty" => return empty(actual),
            "is_not_empty" => return !empty(actual),
            _ => {}
        }
        let Some(actual) = actual else { return false };
        match rule.operator.as_str() {
            "equals" => equal(actual, &rule.value),
            "not_equals" => !equal(actual, &rule.value),
            "contains" | "not_contains" => {
                let found = match actual {
                    Value::String(s) => rule.value.as_str().map(|v| s.contains(v)),
                    Value::Array(v) => Some(v.iter().any(|v| equal(v, &rule.value))),
                    _ => None,
                };
                found.is_some_and(|found| {
                    if rule.operator == "contains" {
                        found
                    } else {
                        !found
                    }
                })
            }
            "starts_with" => actual
                .as_str()
                .is_some_and(|s| s.starts_with(rule.value.as_str().unwrap())),
            "ends_with" => actual
                .as_str()
                .is_some_and(|s| s.ends_with(rule.value.as_str().unwrap())),
            op => {
                if let (Some(a), Some(b)) = (actual.as_number(), rule.value.as_number()) {
                    let cmp = number_cmp(a, b);
                    match op {
                        "greater_than" => cmp.is_gt(),
                        "greater_or_equal" => !cmp.is_lt(),
                        "less_than" => cmp.is_lt(),
                        "less_or_equal" => !cmp.is_gt(),
                        _ => false,
                    }
                } else {
                    false
                }
            }
        }
    };
    if any {
        rules.iter().any(matches)
    } else {
        rules.iter().all(matches)
    }
}
pub fn schema() -> Value {
    let mut schema = json!({"type":"object","properties":{"match":{"enum":["all","any"]},"conditions":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"object","properties":{"field":{"type":"string"},"operator":{"enum":["equals","not_equals","exists","not_exists","is_empty","is_not_empty","contains","not_contains","starts_with","ends_with","greater_than","greater_or_equal","less_than","less_or_equal"]},"value":{},"valueType":{"enum":["string","number","boolean","null","json"]}},"required":["field","operator"],"additionalProperties":false}},"pointer":{"type":"string"},"equals":{}}});
    let rows = schema["properties"]["conditions"].clone();
    schema["properties"]["conditions"] =
        json!({"oneOf":[rows,{"type":"string","description":"JSON-encoded condition row array"}]});
    schema["anyOf"] = json!([{"required":["conditions"]},{"required":["pointer","equals"]}]);
    schema
}

/// Compile collection predicates once, preserving legacy Filter JSON-text values.
pub fn filter(config: &Map<String, Value>, items: &[Value]) -> Result<Vec<Value>, FailureCode> {
    let mut config = config.clone();
    config.remove("arrayPointer");
    if config.contains_key("conditions") {
        let (any, rules) = rules(&config)?;
        Ok(items
            .iter()
            .filter(|item| evaluate_rules(any, &rules, item))
            .cloned()
            .collect())
    } else {
        let pointer = config
            .get("pointer")
            .and_then(Value::as_str)
            .ok_or(FailureCode::InvalidConfig)?;
        let expected = config
            .get("equals")
            .and_then(super::config_json)
            .ok_or(FailureCode::InvalidConfig)?;
        Ok(items
            .iter()
            .filter(|item| item.pointer(pointer) == Some(&expected))
            .cloned()
            .collect())
    }
}
pub fn validate_filter(config: &Map<String, Value>) -> Result<(), FailureCode> {
    if config.get("arrayPointer").is_some_and(|v| !v.is_string()) {
        return bad();
    }
    let mut predicate = config.clone();
    predicate.remove("arrayPointer");
    if predicate.contains_key("conditions") {
        rules(&predicate).map(|_| ())
    } else {
        filter(config, &[]).map(|_| ())
    }
}
