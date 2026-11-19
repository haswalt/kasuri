//! Bounded opt-in request metadata. Credentials never enter the captured context.
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, PartialEq)]
pub enum ContextError {
    InvalidConfig,
    BudgetExceeded,
}
fn list(
    config: &Map<String, Value>,
    field: &str,
    defaults: &[&str],
) -> Result<Vec<String>, ContextError> {
    let Some(value) = config.get(field) else {
        return Ok(defaults.iter().map(|s| s.to_string()).collect());
    };
    let value = match value {
        Value::String(text) => {
            serde_json::from_str(text).map_err(|_| ContextError::InvalidConfig)?
        }
        value => value.clone(),
    };
    let values = value
        .as_array()
        .filter(|v| v.len() <= 32)
        .ok_or(ContextError::InvalidConfig)?;
    values
        .iter()
        .map(|value| {
            let name = value
                .as_str()
                .filter(|s| super::valid_parameter(s))
                .ok_or(ContextError::InvalidConfig)?;
            Ok(name.to_string())
        })
        .collect()
}
pub fn selection(config: &Map<String, Value>) -> Result<(Vec<String>, Vec<String>), ContextError> {
    Ok((
        list(
            config,
            "contextHeaders",
            &["content-type", "x-kasuri-event-id"],
        )?,
        list(config, "contextQuery", &[])?,
    ))
}
fn sensitive(name: &str) -> bool {
    let name = name.to_ascii_lowercase().replace('_', "-");
    [
        "authorization",
        "cookie",
        "password",
        "secret",
        "token",
        "signature",
        "api-key",
        "apikey",
        "credential",
        "session",
    ]
    .iter()
    .any(|word| name.contains(word))
}
pub fn capture(
    config: &Map<String, Value>,
    method: &str,
    path: &str,
    headers: impl IntoIterator<Item = (String, String)>,
    query: impl IntoIterator<Item = (String, String)>,
) -> Result<Value, ContextError> {
    let (selected_headers, selected_query) = selection(config)?;
    let allowed: BTreeSet<_> = selected_headers
        .into_iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    let consumed = if config.get("authentication").and_then(Value::as_str) == Some("query") {
        Some(
            config
                .get("queryParameter")
                .and_then(Value::as_str)
                .unwrap_or("key"),
        )
    } else {
        None
    };
    let mut safe_headers = BTreeMap::new();
    let mut safe_query: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in headers {
        let name = name.to_ascii_lowercase();
        if allowed.contains(&name) && !sensitive(&name) {
            if value.len() > 4096 {
                return Err(ContextError::BudgetExceeded);
            }
            safe_headers.insert(name, value);
        }
    }
    for (name, value) in query {
        if selected_query.contains(&name) && Some(name.as_str()) != consumed && !sensitive(&name) {
            let values = safe_query.entry(name).or_default();
            if value.len() > 4096 || values.len() >= 8 {
                return Err(ContextError::BudgetExceeded);
            }
            values.push(value);
        }
    }
    let context = json!({"trigger":"webhook","request":{"method":method,"path":path,"headers":safe_headers,"query":safe_query}});
    if context.to_string().len() > 16384 {
        return Err(ContextError::BudgetExceeded);
    }
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_metadata_excludes_consumed_credentials_and_preserves_query_multiplicity() {
        let config = json!({"authentication":"query","queryParameter":"custom","contextHeaders":["X-Business-Event","Authorization","Cookie","X-Kasuri-Signature"],"contextQuery":["custom","topic","api_key"]});
        let context = capture(
            config.as_object().unwrap(),
            "POST",
            "/hooks/test",
            [
                ("x-business-event".into(), "event-1".into()),
                ("authorization".into(), "Bearer private".into()),
                ("cookie".into(), "session".into()),
                ("x-kasuri-signature".into(), "private".into()),
            ],
            [
                ("custom".into(), "private".into()),
                ("topic".into(), "one".into()),
                ("topic".into(), "two".into()),
                ("api_key".into(), "private".into()),
            ],
        )
        .unwrap();
        assert_eq!(
            context["request"]["headers"],
            json!({"x-business-event":"event-1"})
        );
        assert_eq!(context["request"]["query"], json!({"topic":["one","two"]}));
        assert!(!context.to_string().contains("private"));
    }
    #[test]
    fn metadata_selection_and_values_are_bounded_and_defaults_are_private() {
        assert_eq!(
            selection(
                json!({"contextHeaders":"[\"x-business-event\"]"})
                    .as_object()
                    .unwrap()
            )
            .unwrap()
            .0,
            vec!["x-business-event"]
        );
        assert!(selection(json!({"contextHeaders":["*"]}).as_object().unwrap()).is_err());
        let default = capture(
            &Map::new(),
            "POST",
            "/hooks/test",
            [
                ("authorization".into(), "private".into()),
                ("x-kasuri-event-id".into(), "event".into()),
            ],
            [("topic".into(), "not opted in".into())],
        )
        .unwrap();
        assert_eq!(default["request"]["query"], json!({}));
        assert_eq!(default["request"]["headers"]["x-kasuri-event-id"], "event");
        assert_eq!(
            capture(
                &Map::new(),
                "POST",
                "/hooks/test",
                [("x-kasuri-event-id".into(), "x".repeat(4097))],
                []
            )
            .unwrap_err(),
            ContextError::BudgetExceeded
        );
    }
}
