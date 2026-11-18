//! Runtime-independent HTTP configuration, policy and response handling.
use connector_sdk::{Failure, FailureCode, NodeManifest, Outcome, Task};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use url::{Host, Url};
pub const MAX_RESPONSE_BYTES: usize = 32_768;
pub const MAX_REQUEST_BYTES: usize = 32_768;
#[derive(Debug, Clone)]
pub struct RequestSpec {
    pub url: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
    pub response_type: String,
}
pub fn failure(code: FailureCode, retryable: bool) -> Outcome {
    Outcome::Failed {
        failure: Failure { code, retryable },
    }
}
pub fn is_read(task: &Task) -> bool {
    matches!(
        task.node
            .config
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("GET")
            .to_ascii_uppercase()
            .as_str(),
        "GET" | "HEAD"
    )
}
fn parse(value: &Value) -> Option<Value> {
    match value {
        Value::String(s) => serde_json::from_str(s).ok(),
        v => Some(v.clone()),
    }
}
fn resolve(value: &Value, input: &Value) -> Option<Value> {
    match value {
        Value::Object(map) if map.len() == 1 && map.contains_key("$input") => {
            input.pointer(map["$input"].as_str()?).cloned()
        }
        Value::Object(map) => Some(Value::Object(
            map.iter()
                .map(|(k, v)| Some((k.clone(), resolve(v, input)?)))
                .collect::<Option<_>>()?,
        )),
        Value::Array(items) => Some(Value::Array(
            items
                .iter()
                .map(|v| resolve(v, input))
                .collect::<Option<_>>()?,
        )),
        v => Some(v.clone()),
    }
}
pub fn validate(task: &Task) -> Result<RequestSpec, FailureCode> {
    let config = &task.node.config;
    let mut url = Url::parse(
        config
            .get("url")
            .and_then(Value::as_str)
            .ok_or(FailureCode::InvalidConfig)?,
    )
    .map_err(|_| FailureCode::InvalidConfig)?;
    // Only public DNS names over HTTPS. Deployment operators additionally approve exact hosts.
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err(FailureCode::InvalidConfig);
    }
    let Some(Host::Domain(host)) = url.host() else {
        return Err(FailureCode::InvalidConfig);
    };
    if !host.contains('.')
        || host.ends_with('.')
        || ["localhost", "local", "internal", "home", "lan", "invalid"]
            .iter()
            .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
    {
        return Err(FailureCode::InvalidConfig);
    }
    let method = config
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_ascii_uppercase();
    if !matches!(
        method.as_str(),
        "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE"
    ) {
        return Err(FailureCode::InvalidConfig);
    }
    // Until external write reconciliation exists, write attempts cannot be retried automatically.
    if !is_read(task) && task.node.retry.max_attempts != 1 {
        return Err(FailureCode::InvalidConfig);
    }
    if let Some(query) = config.get("query").filter(|v| v.as_str() != Some("")) {
        let value = parse(query)
            .and_then(|v| resolve(&v, &task.input))
            .ok_or(FailureCode::InvalidConfig)?;
        let object = value.as_object().ok_or(FailureCode::InvalidConfig)?;
        for (key, value) in object {
            let value = match value {
                Value::String(s) => s.clone(),
                Value::Null | Value::Bool(_) | Value::Number(_) => value.to_string(),
                _ => return Err(FailureCode::InvalidConfig),
            };
            url.query_pairs_mut().append_pair(key, &value);
        }
    }
    let mut headers = BTreeMap::new();
    headers.insert("accept".into(), "application/json".into());
    if let Some(value) = config.get("headers").filter(|v| v.as_str() != Some("")) {
        let value = parse(value).ok_or(FailureCode::InvalidConfig)?;
        for (name, value) in value.as_object().ok_or(FailureCode::InvalidConfig)? {
            let name = name.to_ascii_lowercase();
            // Credentials are intentionally excluded until the encrypted connection vault exists.
            if !matches!(name.as_str(), "accept" | "content-type" | "user-agent") {
                return Err(FailureCode::InvalidConfig);
            }
            let value = value
                .as_str()
                .filter(|s| s.len() <= 1024 && !s.contains(['\r', '\n']))
                .ok_or(FailureCode::InvalidConfig)?;
            headers.insert(name, value.into());
        }
    }
    let body = if let Some(value) = config.get("body").filter(|v| v.as_str() != Some("")) {
        if is_read(task) {
            return Err(FailureCode::InvalidConfig);
        }
        let value = parse(value)
            .and_then(|v| resolve(&v, &task.input))
            .ok_or(FailureCode::InvalidConfig)?;
        let body = value.to_string();
        if body.len() > MAX_REQUEST_BYTES {
            return Err(FailureCode::BudgetExceeded);
        }
        headers.insert("content-type".into(), "application/json".into());
        Some(body)
    } else {
        None
    };
    let response_type = config
        .get("responseType")
        .and_then(Value::as_str)
        .unwrap_or("json");
    if !matches!(response_type, "json" | "text") {
        return Err(FailureCode::InvalidConfig);
    }
    if url.as_str().len() > 8192 {
        return Err(FailureCode::BudgetExceeded);
    }
    Ok(RequestSpec {
        url: url.to_string(),
        method,
        headers,
        body,
        response_type: response_type.into(),
    })
}
pub fn allowed(spec: &RequestSpec, hosts: &str) -> bool {
    let url = Url::parse(&spec.url).unwrap();
    let host = url.host_str().unwrap();
    hosts
        .split(',')
        .map(str::trim)
        .any(|entry| !entry.is_empty() && entry.eq_ignore_ascii_case(host))
}
pub fn response(
    task: &Task,
    spec: &RequestSpec,
    status: u16,
    content_type: Option<&str>,
    bytes: &[u8],
) -> Outcome {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return failure(FailureCode::BudgetExceeded, false);
    }
    if !(200..300).contains(&status) {
        return failure(
            FailureCode::External,
            is_read(task) && (status == 408 || status == 429 || status >= 500),
        );
    }
    let body = if spec.method == "HEAD" || status == 204 || bytes.is_empty() {
        Value::Null
    } else if spec.response_type == "json" {
        match serde_json::from_slice(bytes) {
            Ok(v) => v,
            Err(_) => return failure(FailureCode::InvalidResponse, false),
        }
    } else {
        match String::from_utf8(bytes.to_vec()) {
            Ok(s) => Value::String(s),
            Err(_) => return failure(FailureCode::InvalidResponse, false),
        }
    };
    // No upstream headers such as Set-Cookie or auth tokens are checkpointed.
    let output = json!({"status":status,"contentType":content_type.map(|s|s.chars().take(256).collect::<String>()),"body":body});
    if output.to_string().len() > 65_536 {
        return failure(FailureCode::BudgetExceeded, false);
    }
    Outcome::success(output)
}
pub fn simulate(task: &Task) -> Outcome {
    let spec = match validate(task) {
        Ok(v) => v,
        Err(code) => return failure(code, false),
    };
    let Some(mock) = task.node.config.get("simulationResponse").and_then(parse) else {
        return failure(FailureCode::MissingInput, false);
    };
    let Some(status) = mock
        .get("status")
        .and_then(Value::as_u64)
        .filter(|s| (100..=599).contains(s))
    else {
        return failure(FailureCode::InvalidConfig, false);
    };
    let Some(body) = mock.get("body") else {
        return failure(FailureCode::InvalidConfig, false);
    };
    let bytes = if spec.response_type == "text" {
        match body.as_str() {
            Some(s) => s.as_bytes().to_vec(),
            None => return failure(FailureCode::InvalidConfig, false),
        }
    } else {
        body.to_string().into_bytes()
    };
    response(
        task,
        &spec,
        status as u16,
        Some(if spec.response_type == "json" {
            "application/json"
        } else {
            "text/plain"
        }),
        &bytes,
    )
}
pub fn manifest() -> NodeManifest {
    NodeManifest {
        kind: "HTTP Request".into(),
        version: 1,
        output_ports: vec!["success".into(), "error".into()],
        has_external_side_effects: true,
        config_schema: json!({"type":"object","required":["url"],"properties":{"url":{"type":"string","format":"uri"},"method":{"enum":["GET","HEAD","POST","PUT","PATCH","DELETE"]},"query":{"oneOf":[{"type":"object"},{"type":"string"}]},"headers":{"oneOf":[{"type":"object"},{"type":"string"}]},"body":{},"responseType":{"enum":["json","text"]},"simulationResponse":{}}}),
    }
}
