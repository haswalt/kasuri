//! Shared payloads and pure validation for Cloudflare Email Service.
use connector_sdk::{Failure, FailureCode, NodeManifest, Outcome, Task};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub use mail_parser::{Message as ParsedMessage, MessageParser};

pub const TRIGGER: &str = "Email Trigger";
pub const SEND: &str = "Send Email";

/// The SMTP envelope is supplied by the Worker, separately from the MIME headers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inbound {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub message_id: Option<String>,
    pub automatic: bool,
    pub attachments: Vec<workflow_files::FileReference>,
    pub attachments_skipped: usize,
}

pub fn parse(from: String, to: String, message: &mail_parser::Message<'_>) -> Inbound {
    Inbound {
        automatic: from.is_empty()
            || message.headers().all("Auto-Submitted").any(|header| {
                !String::from_utf8_lossy(header.raw_value())
                    .trim()
                    .eq_ignore_ascii_case("no")
            }),
        from,
        to,
        subject: message.subject().unwrap_or_default().to_string(),
        body: message.body_text(0).unwrap_or_default().into_owned(),
        message_id: message.message_id().and_then(thread_id),
        attachments: Vec::new(),
        attachments_skipped: 0,
    }
}

pub fn valid_address(value: &str) -> bool {
    // Single ASCII SMTP mailbox, without a display name or recipient list.
    value.is_ascii()
        && !value
            .contains(|ch: char| ch.is_ascii_control() || ch.is_whitespace() || "<>,;".contains(ch))
        && email_address::EmailAddress::is_valid(value)
}

fn thread_id(value: &str) -> Option<String> {
    let value = value.trim();
    let bare = value
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
        .unwrap_or(value);
    let (local, domain) = bare.split_once('@')?;
    if local.is_empty()
        || domain.is_empty()
        || bare.len() > 998
        || bare
            .bytes()
            .any(|byte| !byte.is_ascii_graphic() || b"<>(),;\\\"".contains(&byte))
        || domain.contains('@')
    {
        return None;
    }
    Some(format!("<{bare}>"))
}

pub struct Outbound<'a> {
    pub to: &'a str,
    pub subject: &'a str,
    pub text: &'a str,
    pub in_reply_to: Option<String>,
}

pub fn validate_send(config: &Map<String, Value>) -> Result<(), FailureCode> {
    for (key, value) in config {
        let value = value.as_str().ok_or(FailureCode::InvalidConfig)?;
        let valid = match key.as_str() {
            "to" => value.is_empty() || valid_address(value),
            "subject" => valid_subject(value),
            "text" => true,
            "inReplyTo" => value.is_empty() || thread_id(value).is_some(),
            _ => false,
        };
        if !valid {
            return Err(FailureCode::InvalidConfig);
        }
    }
    Ok(())
}

fn valid_subject(value: &str) -> bool {
    !value.chars().any(char::is_control)
}

pub fn prepare(task: &Task) -> Result<Outbound<'_>, FailureCode> {
    validate_send(&task.node.config)?;
    let value = |key| {
        task.node
            .config
            .get(key)
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .or_else(|| task.input.get(key).and_then(Value::as_str))
    };
    let to = value("to")
        .filter(|value| valid_address(value))
        .ok_or(FailureCode::MissingInput)?;
    let subject = value("subject")
        .filter(|value| !value.trim().is_empty() && valid_subject(value))
        .ok_or(FailureCode::MissingInput)?;
    let text = value("text")
        .filter(|value| !value.trim().is_empty())
        .ok_or(FailureCode::MissingInput)?;
    let in_reply_to = value("inReplyTo")
        .filter(|value| !value.is_empty())
        .map(|value| thread_id(value).ok_or(FailureCode::InvalidConfig))
        .transpose()?;
    Ok(Outbound {
        to,
        subject,
        text,
        in_reply_to,
    })
}

pub fn simulate(task: &Task) -> Outcome {
    prepare(task).map_or_else(failed, |_| {
        Outcome::success(json!({"status":"preview","messageId":null}))
    })
}

pub fn failed(code: FailureCode) -> Outcome {
    Outcome::Failed {
        failure: Failure::permanent(code),
    }
}

pub fn inbound_schema() -> Value {
    json!({"type":"object","properties":{
        "from":{"type":"string"},"to":{"type":"string"},"subject":{"type":"string"},
        "body":{"type":"string"},"messageId":{"type":["string","null"]},"automatic":{"type":"boolean"},
        "attachments":{"type":"array","items":workflow_files::FileReference::schema()},
        "attachmentsSkipped":{"type":"integer","minimum":0}
    },"required":["from","to","subject","body","messageId","automatic","attachments","attachmentsSkipped"]})
}
pub fn send_input_schema() -> Value {
    json!({"type":"object","properties":{
        "to":{"type":"string"},"subject":{"type":"string"},"text":{"type":"string"},
        "inReplyTo":{"type":["string","null"]}
    }})
}
pub fn manifests() -> [NodeManifest; 2] {
    [
        NodeManifest {
            kind: TRIGGER.into(),
            version: 1,
            config_schema: json!({"type":"object","additionalProperties":false}),
            output_ports: vec!["success".into()],
            has_external_side_effects: false,
        },
        NodeManifest {
            kind: SEND.into(),
            version: 1,
            config_schema: json!({"type":"object","additionalProperties":false,"properties":send_input_schema()["properties"].as_object().unwrap().keys().map(|key|(key.clone(),json!({"type":"string"}))).collect::<Map<_,_>>()}),
            output_ports: vec!["success".into(), "error".into()],
            has_external_side_effects: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_mime_text_and_uses_the_envelope_for_routing() {
        let raw = b"From: Header <header@example.test>\r\nTo: unrelated@example.test\r\nSubject: =?UTF-8?Q?Caf=C3=A9?=\r\nMessage-ID: <one@example.test>\r\nAuto-Submitted: auto-replied\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nHello=20there";
        let message = MessageParser::new().parse(raw).unwrap();
        let inbound = parse(
            "envelope@example.test".into(),
            "booking@example.test".into(),
            &message,
        );
        assert_eq!(inbound.from, "envelope@example.test");
        assert_eq!(inbound.to, "booking@example.test");
        assert_eq!(inbound.subject, "Café");
        assert_eq!(inbound.body, "Hello there");
        assert_eq!(inbound.message_id.as_deref(), Some("<one@example.test>"));
        assert!(inbound.automatic);
    }

    #[test]
    fn preview_validates_recipients_and_thread_headers() {
        let mut task: Task = serde_json::from_value(json!({
            "id":"send","idempotencyKey":"send","attempt":1,"deadlineMs":30_000,
            "node":{"id":"send","type":SEND,"label":"Send","config":{"subject":"Re: Booking"}},
            "input":{"to":"sender@example.test","subject":"Ignored","text":"Thanks","inReplyTo":"one@example.test"}
        })).unwrap();
        let message = prepare(&task).unwrap();
        assert_eq!(message.subject, "Re: Booking");
        assert_eq!(message.in_reply_to.as_deref(), Some("<one@example.test>"));
        assert_eq!(
            simulate(&task),
            Outcome::success(json!({"status":"preview","messageId":null}))
        );
        task.input["inReplyTo"] = json!("<one@example.test>\r\nBcc: extra@example.test");
        assert_eq!(simulate(&task), failed(FailureCode::InvalidConfig));
    }
}
