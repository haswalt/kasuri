//! Build Sentry error envelopes from a small, explicit workflow data contract.
use connector_sdk::{Failure, FailureCode, NodeManifest, Outcome, Task};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use url::Url;

pub const KIND: &str = "Sentry";

pub fn validate(config: &Map<String, Value>) -> Result<&str, FailureCode> {
    if config
        .keys()
        .any(|key| !matches!(key.as_str(), "project" | "message"))
    {
        return Err(FailureCode::InvalidConfig);
    }
    if config.get("message").is_some_and(|value| {
        value
            .as_str()
            .is_none_or(|message| message.trim().is_empty() || message.len() > 256)
    }) {
        return Err(FailureCode::InvalidConfig);
    }
    config
        .get("project")
        .and_then(Value::as_str)
        .filter(|alias| {
            !alias.is_empty()
                && alias.len() <= 64
                && alias
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
        .ok_or(FailureCode::InvalidConfig)
}

pub fn failure(code: FailureCode) -> Outcome {
    Outcome::Failed {
        failure: Failure::permanent(code),
    }
}

fn report(task: &Task) -> Result<(&str, Option<Value>), FailureCode> {
    validate(&task.node.config)?;
    // Only engine failure codes are accepted from incoming workflow data.
    let code = task.input.pointer("/error/code").and_then(|value| {
        serde_json::from_value::<FailureCode>(value.clone())
            .ok()
            .map(|code| json!(code))
    });
    let message = task
        .node
        .config
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| {
            code.as_ref()
                .and_then(Value::as_str)
                .map(|_| "Kasuri workflow node failed")
        })
        .ok_or(FailureCode::MissingInput)?;
    Ok((message, code))
}

pub fn simulate(task: &Task) -> Outcome {
    match report(task) {
        Ok(_) => Outcome::success(json!({"status":"preview","eventId":null})),
        Err(code) => failure(code),
    }
}

pub struct Envelope {
    pub url: Url,
    pub event_id: String,
    pub body: String,
}

pub fn prepare(task: &Task, dsn: &str, now_ms: u64) -> Result<Envelope, FailureCode> {
    let (message, code) = report(task)?;
    let mut url = Url::parse(dsn).map_err(|_| FailureCode::InvalidConfig)?;
    if url.scheme() != "https"
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host().is_none()
        || url.username().is_empty()
        || !url
            .username()
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(FailureCode::InvalidConfig);
    }
    let (prefix, project) = url
        .path()
        .trim_end_matches('/')
        .rsplit_once('/')
        .filter(|(_, id)| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or(FailureCode::InvalidConfig)?;
    let path = format!("{prefix}/api/{project}/envelope/");
    url.set_username("")
        .map_err(|_| FailureCode::InvalidConfig)?;
    url.set_path(&path);
    let mut digest = Sha256::digest(task.idempotency_key.as_bytes());
    digest[6] = (digest[6] & 0x0f) | 0x40;
    digest[8] = (digest[8] & 0x3f) | 0x80;
    let event_id: String = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let workflow = task.context["workflow"]["id"]
        .as_str()
        .ok_or(FailureCode::MissingInput)?;
    let run = task.context["run"]["id"]
        .as_str()
        .ok_or(FailureCode::MissingInput)?;
    let mut tags = json!({
        "kasuri.workflow_id":workflow.chars().take(200).collect::<String>(),
        "kasuri.run_id":run.chars().take(200).collect::<String>()
    });
    if let Some(code) = &code {
        tags["kasuri.error_code"] = code.clone();
    }
    let event = json!({
        "event_id":event_id, "platform":"other", "level":"error",
        "timestamp":now_ms as f64 / 1000.0, "logentry":{"formatted":message},
        "tags":tags, "extra":{"workflow_id":workflow,"run_id":run},
        "fingerprint":["kasuri",workflow,code.as_ref().and_then(Value::as_str).unwrap_or(message)]
    });
    let header = json!({"event_id":event_id,"dsn":dsn});
    Ok(Envelope {
        url,
        event_id,
        body: format!("{header}\n{{\"type\":\"event\"}}\n{event}\n"),
    })
}

pub fn manifest() -> NodeManifest {
    NodeManifest {
        kind: KIND.into(),
        version: 1,
        config_schema: json!({
            "type":"object",
            "properties":{
                "project":{"type":"string","pattern":"^[A-Za-z0-9_-]{1,64}$"},
                "message":{"type":"string","minLength":1,"maxLength":256}
            },
            "required":["project"],
            "additionalProperties":false
        }),
        output_ports: vec!["success".into(), "error".into()],
        has_external_side_effects: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelopes_use_stable_ids_and_only_safe_run_fields() {
        let task: Task = serde_json::from_value(json!({
            "id":"task","idempotencyKey":"run-1:report","attempt":1,"deadlineMs":5000,
            "node":{"id":"report","type":"Sentry","label":"Report","config":{"project":"errors"}},
            "context":{"workflow":{"id":"workflow-1","name":"private-label"},"run":{"id":"run-1"}},
            "input":{"error":{"code":"invalid_config"},"body":"private-payload"}
        }))
        .unwrap();
        let dsn = "https://fixture@ingest.example.com/sentry/123";
        let envelope = prepare(&task, dsn, 1001).unwrap();
        assert_eq!(
            envelope.url.as_str(),
            "https://ingest.example.com/sentry/api/123/envelope/"
        );
        assert_eq!(envelope.event_id.len(), 32);
        assert_eq!(
            prepare(&task, dsn, 1002).unwrap().event_id,
            envelope.event_id
        );
        assert!(!envelope.body.contains("private-"));
        let event: Value = serde_json::from_str(envelope.body.lines().nth(2).unwrap()).unwrap();
        assert_eq!(event["tags"]["kasuri.error_code"], "invalid_config");
        assert_eq!(event["tags"]["kasuri.run_id"], "run-1");
        assert_eq!(event["timestamp"], 1.001);
        assert!(prepare(&task, "http://fixture@ingest.example.com/123", 1001).is_err());
        assert!(
            prepare(
                &task,
                "https://fixture:password@ingest.example.com/123",
                1001
            )
            .is_err()
        );
    }
}
