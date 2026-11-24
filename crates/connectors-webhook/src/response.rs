//! Bounded response preparation; the host owns the single HTTP delivery window.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
pub const RESPONSE_WINDOW_MS: u64 = 5000;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResponseSpec {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Value,
    pub response_type: String,
}
#[derive(Debug, PartialEq)]
pub enum ResponseError {
    InvalidConfig,
    MissingInput,
    BudgetExceeded,
}
fn decode(value: &Value) -> Result<Value, ResponseError> {
    match value {
        Value::String(s) => serde_json::from_str(s).map_err(|_| ResponseError::InvalidConfig),
        value => Ok(value.clone()),
    }
}
fn charge(value: &Value, budget: &mut usize) -> Result<(), ResponseError> {
    struct Sink<'a>(&'a mut usize);
    impl std::io::Write for Sink<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > *self.0 {
                return Err(std::io::Error::other("Response budget exceeded"));
            }
            *self.0 -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Sink(budget), value).map_err(|_| ResponseError::BudgetExceeded)
}
fn resolve(
    value: &Value,
    input: &Value,
    budget: &mut usize,
    depth: usize,
) -> Result<Value, ResponseError> {
    if depth > 64 {
        return Err(ResponseError::InvalidConfig);
    }
    match value {
        Value::Object(map) if map.contains_key("$input") => {
            if map.len() != 1 {
                return Err(ResponseError::InvalidConfig);
            }
            let pointer = map["$input"].as_str().ok_or(ResponseError::InvalidConfig)?;
            let value = input.pointer(pointer).ok_or(ResponseError::MissingInput)?;
            charge(value, budget)?;
            Ok(value.clone())
        }
        Value::Object(map) => {
            let mut result = Map::new();
            for (key, value) in map {
                charge(&json!(key), budget)?;
                *budget = budget.checked_sub(3).ok_or(ResponseError::BudgetExceeded)?;
                result.insert(key.clone(), resolve(value, input, budget, depth + 1)?);
            }
            Ok(Value::Object(result))
        }
        Value::Array(items) => {
            *budget = budget
                .checked_sub(items.len() + 2)
                .ok_or(ResponseError::BudgetExceeded)?;
            Ok(Value::Array(
                items
                    .iter()
                    .map(|v| resolve(v, input, budget, depth + 1))
                    .collect::<Result<_, _>>()?,
            ))
        }
        value => {
            charge(value, budget)?;
            Ok(value.clone())
        }
    }
}
/// Webhook responses are served from the application's own origin, so a workflow
/// must not be able to redirect, refresh or clear the site, prompt for credentials,
/// pin transport policy or preload resources there.
fn origin_controlling_header(name: &str) -> bool {
    matches!(
        name,
        "location"
            | "content-location"
            | "refresh"
            | "clear-site-data"
            | "link"
            | "www-authenticate"
            | "strict-transport-security"
            | "alt-svc"
            | "service-worker-allowed"
            | "set-login"
            | "speculation-rules"
            | "supports-loading-mode"
    ) || name.starts_with("x-kasuri-")
        || name.starts_with("proxy-")
}
/// Statuses a workflow may send. Redirects (3xx other than 304) are refused because
/// they would turn the application origin into an open redirect.
pub fn allowed_status(status: u64) -> bool {
    (200..=599).contains(&status) && (!(300..=399).contains(&status) || status == 304)
}
pub fn blocked_header(name: &str) -> bool {
    origin_controlling_header(name)
        || matches!(
            name,
            "set-cookie"
                | "set-cookie2"
                | "cookie"
                | "connection"
                | "transfer-encoding"
                | "content-type"
                | "content-length"
                | "content-encoding"
                | "host"
                | "upgrade"
                | "keep-alive"
                | "te"
                | "trailer"
                | "proxy-connection"
                | "proxy-authenticate"
                | "proxy-authorization"
                | "x-kasuri-run-id"
                | "cache-control"
                | "x-content-type-options"
        )
}
fn valid_header(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
        && !blocked_header(&name.to_ascii_lowercase())
}
fn status_value(value: &Value) -> Result<Value, ResponseError> {
    if value
        .as_str()
        .is_some_and(|s| s.trim_start().starts_with('{'))
    {
        decode(value)
    } else {
        Ok(value.clone())
    }
}
pub fn validate(config: &Map<String, Value>) -> Result<(), ResponseError> {
    if config
        .keys()
        .any(|k| !matches!(k.as_str(), "status" | "headers" | "body" | "responseType"))
        || !matches!(
            config
                .get("responseType")
                .and_then(Value::as_str)
                .unwrap_or("json"),
            "json" | "text"
        )
    {
        return Err(ResponseError::InvalidConfig);
    }
    if let Some(value) = config.get("status") {
        let value = status_value(value)?;
        if !value.is_object()
            && !value
                .as_u64()
                .or_else(|| value.as_str()?.parse().ok())
                .is_some_and(allowed_status)
        {
            return Err(ResponseError::InvalidConfig);
        }
    }
    if let Some(headers) = config.get("headers").filter(|v| v.as_str() != Some("")) {
        let headers = decode(headers)?;
        let headers = headers.as_object().ok_or(ResponseError::InvalidConfig)?;
        if headers.len() > 16 || headers.keys().any(|k| !valid_header(k)) {
            return Err(ResponseError::InvalidConfig);
        }
    }
    Ok(())
}
pub fn prepare(config: &Map<String, Value>, input: &Value) -> Result<ResponseSpec, ResponseError> {
    validate(config)?;
    let mut budget = 32768;
    let status = resolve(
        &status_value(config.get("status").unwrap_or(&json!(200)))?,
        input,
        &mut budget,
        0,
    )?;
    let status = status
        .as_u64()
        .or_else(|| status.as_str()?.parse().ok())
        .filter(|s| allowed_status(*s))
        .ok_or(ResponseError::InvalidConfig)? as u16;
    let response_type = config
        .get("responseType")
        .and_then(Value::as_str)
        .unwrap_or("json");
    let body = config
        .get("body")
        .cloned()
        .unwrap_or(if response_type == "text" {
            json!("")
        } else {
            Value::Null
        });
    let body = if response_type == "json" {
        decode(&body)?
    } else {
        body
    };
    let body = resolve(&body, input, &mut budget, 0)?;
    if response_type == "text" && !body.is_string() {
        return Err(ResponseError::InvalidConfig);
    }
    if matches!(status, 204 | 205 | 304) && !(body.is_null() || body.as_str() == Some("")) {
        return Err(ResponseError::InvalidConfig);
    }
    let mut headers = BTreeMap::new();
    let mut header_budget = 8192;
    if let Some(value) = config.get("headers").filter(|v| v.as_str() != Some("")) {
        let values = resolve(&decode(value)?, input, &mut header_budget, 0)?;
        let values = values.as_object().ok_or(ResponseError::InvalidConfig)?;
        if values.len() > 16 || values.keys().any(|name| !valid_header(name)) {
            return Err(ResponseError::InvalidConfig);
        }
        for (name, value) in values {
            let value = value
                .as_str()
                .filter(|v| {
                    v.len() <= 4096 && v.bytes().all(|b| b == b'\t' || (32..=126).contains(&b))
                })
                .ok_or(ResponseError::InvalidConfig)?;
            if headers
                .insert(name.to_ascii_lowercase(), value.into())
                .is_some()
            {
                return Err(ResponseError::InvalidConfig);
            }
        }
    }
    Ok(ResponseSpec {
        status,
        headers,
        body,
        response_type: response_type.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_references_and_editor_json_prepare_safe_responses() {
        let config = json!({"status":"201","headers":"{\"X-Result\":{\"$input\":\"/name\"}}","body":"{\"accepted\":{\"$input\":\"/accepted\"}}"});
        let response = prepare(
            config.as_object().unwrap(),
            &json!({"name":"created","accepted":true}),
        )
        .unwrap();
        assert_eq!(response.status, 201);
        assert_eq!(response.headers["x-result"], "created");
        assert_eq!(response.body, json!({"accepted":true}));
        assert_eq!(
            prepare(
                json!({"responseType":"text","body":{"$input":"/name"}})
                    .as_object()
                    .unwrap(),
                &json!({"name":"hello"})
            )
            .unwrap()
            .body,
            "hello"
        );
    }
    #[test]
    fn headers_status_and_input_expansion_are_bounded_before_delivery() {
        for config in [
            json!({"headers":{"Set-Cookie":"session=bad"}}),
            json!({"headers":{"Connection":"close"}}),
            json!({"headers":{"Content-Length":"99"}}),
            json!({"headers":{"Content-Type":"text/html"}}),
            json!({"status":101}),
            json!({"status":204,"body":{"unexpected":true}}),
            json!({"headers":{"X-Test":"a\r\nb"}}),
            json!({"status":302,"headers":{"Location":"https://example.com"}}),
            json!({"status":301}),
            json!({"status":"307"}),
            json!({"status":308}),
            json!({"headers":{"Location":"https://example.com"}}),
            json!({"headers":{"Clear-Site-Data":"\"cookies\""}}),
            json!({"headers":{"Refresh":"0; url=https://example.com"}}),
            json!({"headers":{"Link":"<https://example.com/x.js>; rel=preload"}}),
            json!({"headers":{"WWW-Authenticate":"Basic realm=\"Kasuri\""}}),
            json!({"headers":{"Strict-Transport-Security":"max-age=0"}}),
            json!({"headers":{"X-Kasuri-Session":"forged"}}),
        ] {
            assert!(
                prepare(config.as_object().unwrap(), &json!({})).is_err(),
                "{config}"
            );
        }
        let references: Vec<_> = (0..100).map(|_| json!({"$input":"/large"})).collect();
        assert_eq!(
            prepare(
                json!({"body":references}).as_object().unwrap(),
                &json!({"large":"x".repeat(1024)})
            )
            .unwrap_err(),
            ResponseError::BudgetExceeded
        );
        assert_eq!(
            prepare(
                json!({"body":{"$input":"/missing"}}).as_object().unwrap(),
                &json!({})
            )
            .unwrap_err(),
            ResponseError::MissingInput
        );
    }
}

#[cfg(test)]
mod dynamic_header_tests {
    use super::*;
    #[test]
    fn resolved_header_objects_cannot_inject_platform_or_framing_headers() {
        for name in [
            "Set-Cookie",
            "Content-Type",
            "Connection",
            "Content-Length",
            "X-Kasuri-Run-Id",
        ] {
            let input = json!({name:"injected"});
            assert!(
                prepare(
                    json!({"headers":{"$input":""}}).as_object().unwrap(),
                    &input
                )
                .is_err()
            );
        }
        assert!(
            prepare(
                json!({"headers":{"$input":""}}).as_object().unwrap(),
                &json!({"X-Safe":"ok"})
            )
            .is_ok()
        );
        for name in ["Location", "Clear-Site-Data", "Refresh"] {
            assert!(
                prepare(
                    json!({"headers":{"$input":""}}).as_object().unwrap(),
                    &json!({name:"injected"})
                )
                .is_err(),
                "{name}"
            );
        }
        let redirect = json!({"status":{"$input":"/status"}});
        assert!(prepare(redirect.as_object().unwrap(), &json!({"status":302})).is_err());
        assert_eq!(
            prepare(json!({"status":304}).as_object().unwrap(), &json!({}))
                .unwrap()
                .status,
            304
        );
        assert!(
            prepare(
                json!({"headers":{"Retry-After":"30","X-Request-Id":"abc"}})
                    .as_object()
                    .unwrap(),
                &json!({})
            )
            .is_ok()
        );
        let dynamic = json!({"status":"{\"$input\":\"/status\"}"});
        assert_eq!(
            prepare(dynamic.as_object().unwrap(), &json!({"status":202}))
                .unwrap()
                .status,
            202
        );
    }
}
