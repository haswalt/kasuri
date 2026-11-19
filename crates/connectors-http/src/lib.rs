//! Runtime-independent HTTP configuration, policy and response handling.
use base64::{Engine, engine::general_purpose::STANDARD};
use connector_sdk::{Failure, FailureCode, NodeManifest, Outcome, Task};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};
use url::{Host, Url};
pub const MAX_RESPONSE_BYTES: usize = 32_768;
pub const MAX_REQUEST_BYTES: usize = 32_768;
#[derive(Clone)]
pub struct RequestSpec {
    pub url: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub secret_headers: BTreeMap<String, String>,
    pub basic: Option<(String, String)>,
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
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.host().is_none()
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
    let mut secret_headers = BTreeMap::new();
    headers.insert("accept".into(), "application/json".into());
    if let Some(value) = config.get("headers").filter(|v| v.as_str() != Some("")) {
        let value = parse(value).ok_or(FailureCode::InvalidConfig)?;
        for (name, value) in value.as_object().ok_or(FailureCode::InvalidConfig)? {
            let name = name.to_ascii_lowercase();
            if !valid_header_name(&name)
                || reserved_header(&name)
                || headers.contains_key(&name) && name != "accept"
                || secret_headers.contains_key(&name)
            {
                return Err(FailureCode::InvalidConfig);
            }
            if let Some(alias) = value
                .as_object()
                .filter(|v| v.len() == 1)
                .and_then(|v| v.get("$secret"))
                .and_then(Value::as_str)
            {
                if !valid_secret_name(alias) {
                    return Err(FailureCode::InvalidConfig);
                }
                secret_headers.insert(name, alias.into());
            } else {
                if sensitive_header(&name) {
                    return Err(FailureCode::InvalidConfig);
                }
                let value = value
                    .as_str()
                    .filter(|s| valid_header_value(s))
                    .ok_or(FailureCode::InvalidConfig)?;
                headers.insert(name, value.into());
            }
        }
    }
    let basic = match config
        .get("authentication")
        .and_then(Value::as_str)
        .unwrap_or("none")
    {
        "none" | "" => None,
        "basic" => {
            if headers.contains_key("authorization") || secret_headers.contains_key("authorization")
            {
                return Err(FailureCode::InvalidConfig);
            }
            let username = config
                .get("username")
                .and_then(Value::as_str)
                .filter(|s| !s.contains(':') && valid_header_value(s))
                .ok_or(FailureCode::InvalidConfig)?;
            let password = config
                .get("passwordSecret")
                .and_then(Value::as_str)
                .filter(|s| valid_secret_name(s))
                .ok_or(FailureCode::InvalidConfig)?;
            Some((username.into(), password.into()))
        }
        _ => return Err(FailureCode::InvalidConfig),
    };
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
        if !secret_headers.contains_key("content-type") {
            headers
                .entry("content-type".into())
                .or_insert_with(|| "application/json".into());
        }
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
        secret_headers,
        basic,
        body,
        response_type: response_type.into(),
    })
}
fn listed(host: &str, hosts: &str) -> bool {
    hosts
        .split(',')
        .map(str::trim)
        .any(|entry| !entry.is_empty() && entry.eq_ignore_ascii_case(host.trim_matches(['[', ']'])))
}
pub fn allowed(spec: &RequestSpec, hosts: &str) -> bool {
    hosts.trim().is_empty() || listed(Url::parse(&spec.url).unwrap().host_str().unwrap(), hosts)
}
pub fn private_opt_in(spec: &RequestSpec, hosts: &str) -> bool {
    listed(Url::parse(&spec.url).unwrap().host_str().unwrap(), hosts)
}
pub fn public_destination(spec: &RequestSpec) -> bool {
    match Url::parse(&spec.url).unwrap().host().unwrap() {
        Host::Ipv4(ip) => public_ip(IpAddr::V4(ip)),
        Host::Ipv6(ip) => public_ip(IpAddr::V6(ip)),
        Host::Domain(host) => {
            host.contains('.')
                && !host.ends_with('.')
                && ![
                    "localhost",
                    "local",
                    "internal",
                    "home",
                    "lan",
                    "invalid",
                    "test",
                    "onion",
                    "arpa",
                ]
                .iter()
                .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
                && !matches!(
                    host,
                    "metadata.google" | "metadata.azure.com" | "instance-data.ec2.internal"
                )
        }
    }
}
fn v4_public(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(matches!(a, 0 | 10 | 127)
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && (b == 168 || b == 0 || b == 2))
        || (a == 198 && (b == 18 || b == 19 || b == 51 && c == 100))
        || (a == 203 && b == 0 && c == 113))
}
fn v6_public(ip: Ipv6Addr) -> bool {
    if let Some(ip) = ip.to_ipv4_mapped() {
        return v4_public(ip);
    }
    let segments = ip.segments();
    // Only global unicast; exclude special-use, transition and documentation networks.
    (segments[0] & 0xe000) == 0x2000
        && !(segments[0] == 0x2001 && segments[1] < 0x200)
        && !(segments[0] == 0x2001 && segments[1] == 0xdb8)
        && segments[0] != 0x2002
        && !(segments[0] == 0x3fff && (segments[1] & 0xf000) == 0)
}
pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => v4_public(ip),
        IpAddr::V6(ip) => v6_public(ip),
    }
}
fn valid_secret_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}
fn valid_header_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}
fn valid_header_value(s: &str) -> bool {
    s.len() <= 8192 && s.bytes().all(|b| b == b'\t' || b >= 32 && b != 127)
}
fn reserved_header(s: &str) -> bool {
    matches!(
        s,
        "host"
            | "content-length"
            | "transfer-encoding"
            | "connection"
            | "keep-alive"
            | "te"
            | "trailer"
            | "upgrade"
            | "idempotency-key"
    ) || s.starts_with("proxy-")
        || s.starts_with("cf-")
        || s.starts_with("sec-")
        || s.starts_with("x-forwarded-")
        || s == "forwarded"
}
fn sensitive_header(s: &str) -> bool {
    matches!(s, "authorization" | "cookie" | "x-api-key" | "api-key")
        || s.contains("token")
        || s.contains("secret")
        || s.ends_with("-key")
}
/// Resolve only in the network host, after checkpoints are saved. Never serialize this result.
pub fn authorization_headers(
    spec: &RequestSpec,
    secrets: &BTreeMap<String, String>,
) -> Result<(BTreeMap<String, String>, Vec<String>), FailureCode> {
    let mut headers = spec.headers.clone();
    let mut redactions = Vec::new();
    for (name, alias) in &spec.secret_headers {
        let value = secrets
            .get(alias)
            .filter(|s| valid_header_value(s))
            .ok_or(FailureCode::MissingInput)?;
        headers.insert(name.clone(), value.clone());
        redactions.push(value.clone());
        if name == "cookie" {
            for part in value.split(';') {
                if let Some((_, token)) = part.trim().split_once('=') {
                    redactions.push(token.to_string());
                }
            }
        }
        if name == "authorization" {
            if let Some((scheme, token)) = value.split_once(' ') {
                redactions.push(token.to_string());
                if scheme.eq_ignore_ascii_case("basic") {
                    if let Some(pair) = STANDARD
                        .decode(token)
                        .ok()
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                    {
                        if let Some((_, password)) = pair.split_once(':') {
                            redactions.push(password.to_string());
                        }
                        redactions.push(pair);
                    }
                }
            }
        }
    }
    if let Some((username, alias)) = &spec.basic {
        let password = secrets.get(alias).ok_or(FailureCode::MissingInput)?;
        let pair = format!("{username}:{password}");
        let encoded = STANDARD.encode(pair.as_bytes());
        let value = format!("Basic {encoded}");
        if !valid_header_value(&value) {
            return Err(FailureCode::InvalidConfig);
        }
        headers.insert("authorization".into(), value.clone());
        redactions.extend([password.clone(), pair, encoded, value]);
    }
    redactions.retain(|s| !s.is_empty());
    redactions.sort_by_key(|s| std::cmp::Reverse(s.len()));
    Ok((headers, redactions))
}
fn redact_text(text: &str, secrets: &[String]) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    while cursor < text.len() {
        if let Some(secret) = secrets
            .iter()
            .filter(|s| !s.is_empty())
            .find(|s| text[cursor..].starts_with(s.as_str()))
        {
            output.push_str("[REDACTED]");
            cursor += secret.len();
        } else {
            let next = text[cursor..].chars().next().unwrap();
            output.push(next);
            cursor += next.len_utf8();
        }
    }
    output
}
pub fn redact(output: &mut Value, secrets: &[String]) {
    match output {
        Value::String(value) => *value = redact_text(value, secrets),
        Value::Array(items) => {
            for item in items {
                redact(item, secrets)
            }
        }
        Value::Object(map) => {
            let original = std::mem::take(map);
            for (key, mut value) in original {
                redact(&mut value, secrets);
                map.insert(redact_text(&key, secrets), value);
            }
        }
        value @ (Value::Number(_) | Value::Bool(_)) => {
            if secrets.iter().any(|s| *s == value.to_string()) {
                *value = Value::String("[REDACTED]".into());
            }
        }
        _ => {}
    }
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
        config_schema: json!({"type":"object","required":["url"],"properties":{"url":{"type":"string","format":"uri"},"method":{"anyOf":[{"enum":["GET","HEAD","POST","PUT","PATCH","DELETE"]},{"type":"string","pattern":"^(?:[Gg][Ee][Tt]|[Hh][Ee][Aa][Dd]|[Pp][Oo][Ss][Tt]|[Pp][Uu][Tt]|[Pp][Aa][Tt][Cc][Hh]|[Dd][Ee][Ll][Ee][Tt][Ee])$"}],"description":"HTTP method; uppercase recommended, case-insensitive for compatibility"},"query":{"oneOf":[{"type":"object"},{"type":"string"}]},"headers":{"oneOf":[{"type":"object"},{"type":"string"}]},"body":{},"authentication":{"enum":["none","basic"]},"username":{"type":"string"},"passwordSecret":{"type":"string"},"responseType":{"enum":["json","text"]},"simulationResponse":{}}}),
    }
}
