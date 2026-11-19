//! Offline bounded body decoding. Bad data is a normal result for workflow routing.
use connector_sdk::{FailureCode, Outcome};
use serde_json::{Map, Value, json};
const MAX_BYTES: usize = 32_768;
pub fn validate(config: &Map<String, Value>) -> Result<(), FailureCode> {
    if config
        .keys()
        .any(|k| !matches!(k.as_str(), "format" | "pointer"))
        || !matches!(
            config.get("format").and_then(Value::as_str),
            Some("json" | "xml" | "text" | "form")
        )
        || config.get("pointer").is_some_and(|v| {
            !v.is_string()
                || (!v.as_str().unwrap().is_empty() && !v.as_str().unwrap().starts_with('/'))
        })
    {
        return Err(FailureCode::InvalidConfig);
    }
    Ok(())
}
fn decode(text: &str) -> Result<String, &'static str> {
    let bytes = text.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => result.push(b' '),
            b'%' => {
                if i + 2 >= bytes.len() {
                    return Err("invalid_form");
                }
                let hex = |c: u8| (c as char).to_digit(16).map(|x| x as u8);
                let a = hex(bytes[i + 1]).ok_or("invalid_form")?;
                let b = hex(bytes[i + 2]).ok_or("invalid_form")?;
                result.push(a * 16 + b);
                i += 2;
            }
            c => result.push(c),
        };
        i += 1;
    }
    String::from_utf8(result).map_err(|_| "invalid_utf8")
}
fn form(text: &str) -> Result<Value, &'static str> {
    let mut output = Map::new();
    if text.is_empty() {
        return Ok(Value::Object(output));
    }
    for (i, part) in text.split('&').enumerate() {
        if i >= 256 {
            return Err("too_many_fields");
        }
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        let key = decode(key)?;
        let value = Value::String(decode(value)?);
        output
            .entry(key)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap()
            .push(value);
    }
    Ok(Value::Object(output))
}
fn element(node: roxmltree::Node<'_, '_>, depth: usize) -> Result<Value, &'static str> {
    if depth > 32 {
        return Err("too_deep");
    }
    let mut children = Vec::new();
    for child in node.children() {
        if child.is_element() {
            children.push(element(child, depth + 1)?)
        } else if child.is_text() {
            children.push(json!(child.text().unwrap_or("")))
        }
    }
    Ok(
        json!({"name":node.tag_name().name(),"namespace":node.tag_name().namespace(),"attributes":node.attributes().map(|a|json!({"name":a.name(),"namespace":a.namespace(),"value":a.value()})).collect::<Vec<_>>(),"children":children}),
    )
}
fn parse(format: &str, input: &Value) -> Result<Value, &'static str> {
    if input.to_string().len() > MAX_BYTES {
        return Err("body_too_large");
    }
    if format == "json" {
        return match input {
            Value::String(s) => serde_json::from_str::<Value>(s).map_err(|_| "invalid_json"),
            v => Ok(v.clone()),
        };
    }
    let text = input.as_str().ok_or("expected_text")?;
    match format {
        "text" => Ok(input.clone()),
        "form" => form(text),
        "xml" => {
            // Reject even an empty DTD, and never install an entity resolver.
            if text.contains("<!DOCTYPE") {
                return Err("dtd_forbidden");
            }
            let doc = roxmltree::Document::parse_with_options(
                text,
                roxmltree::ParsingOptions {
                    allow_dtd: false,
                    nodes_limit: 1024,
                    entity_resolver: None,
                },
            )
            .map_err(|_| "invalid_xml")?;
            element(doc.root_element(), 0)
        }
        _ => Err("invalid_format"),
    }
}
pub fn execute(config: &Map<String, Value>, input: &Value) -> Outcome {
    if let Err(code) = validate(config) {
        return Outcome::Failed {
            failure: connector_sdk::Failure::permanent(code),
        };
    }
    let pointer = config.get("pointer").and_then(Value::as_str).unwrap_or("");
    let result = input
        .pointer(pointer)
        .ok_or("missing_input")
        .and_then(|v| parse(config["format"].as_str().unwrap(), v));
    let output = match result {
        Ok(value) => json!({"valid":true,"value":value,"errors":[]}),
        Err(code) => json!({"valid":false,"value":null,"errors":[{"code":code}]}),
    };
    if output.to_string().len() > 65_536 {
        return Outcome::success(
            json!({"valid":false,"value":null,"errors":[{"code":"output_too_large"}]}),
        );
    }
    Outcome::success(output)
}
