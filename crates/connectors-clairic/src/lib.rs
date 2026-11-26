//! Typed Clairic requests and results shared by the Worker and offline simulations.
use chrono::NaiveDate;
use connector_sdk::{Failure, FailureCode, NodeManifest, Outcome, Task};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};
use workflow_files::{FileReference, MAX_MEDIA_BYTES, MAX_MEDIA_ITEMS};

pub const KIND: &str = "Clairic";
pub const MODEL: &str = "@cf/cloudflare/clairic";
// Match the engine's existing limit for a single node output.
const MAX_OUTPUT_BYTES: usize = 65_536;

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum FieldType {
    Text,
    Number,
    Date,
    Boolean,
    Enum,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Field {
    #[serde(skip_serializing)]
    name: String,
    #[serde(rename = "type")]
    kind: FieldType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
    #[serde(rename = "enum", default, skip_serializing_if = "Option::is_none")]
    choices: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[serde(default, rename = "inputPointer")]
    input_pointer: Option<String>,
    #[serde(default, rename = "attachmentsPointer")]
    attachments_pointer: Option<String>,
    fields: Vec<Field>,
    #[serde(default, rename = "simulationResponse")]
    simulation_response: Option<ModelResponse>,
    #[serde(default, rename = "referenceDate")]
    reference_date: Option<NaiveDate>,
}

#[derive(Deserialize)]
struct ModelResponse {
    answers: Map<String, Value>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Serialize)]
pub struct Request<'a> {
    model: &'static str,
    state: &'a str,
    fields: BTreeMap<&'a str, &'a Field>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reference_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    images: Vec<&'a MediaPart>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    documents: Vec<&'a MediaPart>,
}

#[derive(Serialize)]
pub struct MediaPart {
    pub content_type: String,
    pub base64: String,
}

fn invalid(path: impl Into<String>, message: &'static str) -> Failure {
    Failure::permanent(FailureCode::InvalidConfig).with_detail(path, message)
}

fn parse(config: &Map<String, Value>) -> Result<Config, Failure> {
    let parsed: Config =
        serde_path_to_error::deserialize(Value::Object(config.clone())).map_err(|error| {
            use serde_path_to_error::Segment;
            let mut path = String::new();
            for segment in error.path() {
                let part = match segment {
                    Segment::Map { key } => key.replace('~', "~0").replace('/', "~1"),
                    Segment::Seq { index } => index.to_string(),
                    _ => break,
                };
                path.push('/');
                path.push_str(&part);
            }
            let message = match path.rsplit('/').next().unwrap_or_default() {
                "type" => "Choose a Clairic field type: text, number, date, boolean or enum.",
                "name" => "Set the extraction field name to a string.",
                "instructions" => "Set extraction instructions to a string.",
                "referenceDate" => "Use a valid reference date in YYYY-MM-DD format.",
                "inputPointer" | "attachmentsPointer" => {
                    "Set the input selector to a JSON Pointer string."
                }
                "fields" => {
                    "Provide an array of extraction fields with a name and type for each entry."
                }
                _ if path.starts_with("/fields/") => {
                    "Check this extraction field's required names, types and options."
                }
                _ if path.starts_with("/simulationResponse") => {
                    "Use a Clairic simulation response with an answers object."
                }
                _ => "Provide extraction fields and use only supported Clairic settings.",
            };
            invalid(path, message)
        })?;
    if parsed.fields.is_empty() {
        return Err(invalid("/fields", "Add at least one extraction field."));
    }
    let mut names = HashSet::new();
    for (index, entry) in parsed.fields.iter().enumerate() {
        let mut bytes = entry.name.bytes();
        if !bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
            || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(invalid(
                format!("/fields/{index}/name"),
                "Start the field name with an ASCII letter and use only letters, digits and underscores.",
            ));
        }
        if !names.insert(&entry.name) {
            return Err(invalid(
                format!("/fields/{index}/name"),
                "Extraction field names must be unique.",
            ));
        }
        let problem = match (entry.kind, &entry.choices) {
            (FieldType::Enum, Some(choices)) => {
                let mut unique = HashSet::new();
                (choices.is_empty()
                    || !choices
                        .iter()
                        .all(|label| !label.trim().is_empty() && unique.insert(label)))
                .then_some("Provide distinct, non-blank labels for this enum field.")
            }
            (FieldType::Enum, None) => Some("Provide allowed labels for this enum field."),
            (_, Some(_)) => Some("Only enum fields accept allowed labels."),
            (_, None) => None,
        };
        if let Some(message) = problem {
            return Err(invalid(format!("/fields/{index}/enum"), message));
        }
    }
    for (path, pointer) in [
        ("/inputPointer", &parsed.input_pointer),
        ("/attachmentsPointer", &parsed.attachments_pointer),
    ] {
        if let Some(pointer) = pointer
            && !pointer.is_empty()
            && (!pointer.starts_with('/')
                || !pointer
                    .split('~')
                    .skip(1)
                    .all(|part| part.starts_with('0') || part.starts_with('1')))
        {
            return Err(invalid(
                path,
                "Use a JSON Pointer starting with /, escaping ~ as ~0 and / as ~1. Leave it empty to select the whole input.",
            ));
        }
    }
    Ok(parsed)
}

pub fn validate(config: &Map<String, Value>) -> Result<(), Failure> {
    let config = parse(config)?;
    if let Some(fixture) = &config.simulation_response {
        normalize(&config, fixture).map_err(|failure| match failure.detail {
            Some(detail) => Failure {
                detail: Some(connector_sdk::FailureDetail {
                    path: format!("/simulationResponse{}", detail.path),
                    message: detail.message,
                }),
                ..Failure::permanent(FailureCode::InvalidConfig)
            },
            None => invalid(
                "/simulationResponse",
                "The simulation response exceeds the node output limit.",
            ),
        })?;
    }
    Ok(())
}

pub struct Prepared<'a> {
    config: Config,
    text: &'a str,
    files: Vec<FileReference>,
}

impl Prepared<'_> {
    pub fn files(&self) -> &[FileReference] {
        &self.files
    }

    pub fn request<'a>(&'a self, media: &'a [MediaPart]) -> Request<'a> {
        let (documents, images) = media
            .iter()
            .partition(|part| part.content_type == "application/pdf");
        Request {
            model: "clairic",
            state: self.text,
            fields: self
                .config
                .fields
                .iter()
                .map(|entry| (entry.name.as_str(), entry))
                .collect(),
            reference_date: self.config.reference_date,
            images,
            documents,
        }
    }

    pub fn complete(&self, response: Value) -> Outcome {
        serde_json::from_value::<ModelResponse>(response)
            .map_err(|_| {
                Failure::permanent(FailureCode::InvalidResponse).with_detail(
                    "",
                    "Workers AI returned a Clairic response without a valid answers object.",
                )
            })
            .and_then(|response| normalize(&self.config, &response))
            .map_or_else(|failure| Outcome::Failed { failure }, Outcome::success)
    }
}

pub fn prepare(task: &Task) -> Result<Prepared<'_>, Failure> {
    let config = parse(&task.node.config)?;
    let files: Vec<FileReference> = match config
        .attachments_pointer
        .as_ref()
        .and_then(|p| task.input.pointer(p))
    {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(files)) if files.len() > MAX_MEDIA_ITEMS => {
            return Err(Failure::permanent(FailureCode::BudgetExceeded));
        }
        Some(list) => serde_json::from_value(list.clone()).map_err(|_| {
            invalid(
                "/attachmentsPointer",
                "The selected input must be an array of private file references.",
            )
        })?,
    };
    let mut ids = HashSet::new();
    if !files
        .iter()
        .all(|file| file.valid() && ids.insert(&file.id))
    {
        return Err(invalid(
            "/attachmentsPointer",
            "The selected attachments contain an invalid or duplicate file reference.",
        ));
    }
    if files.iter().map(|file| file.size).sum::<u64>() > MAX_MEDIA_BYTES {
        return Err(Failure::permanent(FailureCode::BudgetExceeded));
    }
    let pointer = config.input_pointer.as_deref().unwrap_or("");
    let text = match task.input.pointer(pointer) {
        Some(Value::String(text)) => text,
        None | Some(Value::Null) if !files.is_empty() => "",
        Some(_) if config.input_pointer.is_none() && !files.is_empty() => "",
        _ => {
            return Err(Failure::permanent(FailureCode::MissingInput).with_detail(
                "/inputPointer",
                "The selected input must contain non-empty text or valid attachments.",
            ));
        }
    };
    if text.trim().is_empty() && files.is_empty() {
        return Err(Failure::permanent(FailureCode::MissingInput).with_detail(
            "/inputPointer",
            "The selected input must contain non-empty text or valid attachments.",
        ));
    }
    Ok(Prepared {
        config,
        text,
        files,
    })
}

pub fn simulate(task: &Task) -> Outcome {
    prepare(task)
        .and_then(|prepared| {
            let fixture = prepared
                .config
                .simulation_response
                .as_ref()
                .ok_or_else(|| {
                    invalid(
                        "/simulationResponse",
                        "Add a Clairic response fixture to run an offline simulation.",
                    )
                })?;
            normalize(&prepared.config, fixture)
        })
        .map_or_else(|failure| Outcome::Failed { failure }, Outcome::success)
}

pub fn failed(code: FailureCode) -> Outcome {
    Outcome::Failed {
        failure: Failure::permanent(code),
    }
}

fn normalize(config: &Config, response: &ModelResponse) -> Result<Value, Failure> {
    let mut values = Map::new();
    let mut confidence = Map::new();
    for entry in &config.fields {
        let invalid_answer = |suffix: &str, message| {
            Failure::permanent(FailureCode::InvalidResponse)
                .with_detail(format!("/answers/{}{suffix}", entry.name), message)
        };
        let answer = response
            .answers
            .get(&entry.name)
            .and_then(Value::as_object)
            .ok_or_else(|| {
                invalid_answer(
                    "",
                    "Provide an answer object for each configured extraction field.",
                )
            })?;
        let incomplete = answer.get("valid").and_then(Value::as_bool) == Some(false)
            || answer.get("complete").and_then(Value::as_bool) == Some(false);
        let value = if incomplete {
            &Value::Null
        } else {
            answer.get("value").ok_or_else(|| {
                invalid_answer("/value", "Provide a value or mark this answer incomplete.")
            })?
        };
        let valid = value.is_null()
            || match entry.kind {
                FieldType::Text => value.is_string(),
                FieldType::Number => value.is_number(),
                FieldType::Date => value.as_str().is_some_and(|date| {
                    date.len() == 10 && NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok()
                }),
                FieldType::Boolean => value.is_boolean(),
                FieldType::Enum => value.as_str().is_some_and(|value| {
                    entry
                        .choices
                        .as_ref()
                        .unwrap()
                        .iter()
                        .any(|choice| choice == value)
                }),
            };
        if !valid {
            return Err(invalid_answer(
                "/value",
                "The answer must match the extraction field type or allowed labels.",
            ));
        }
        let score = answer
            .get("confidence")
            .filter(|value| !value.is_null())
            .map(|value| {
                value
                    .as_f64()
                    .filter(|score| (0.0..=1.0).contains(score))
                    .ok_or_else(|| {
                        invalid_answer(
                            "/confidence",
                            "Confidence must be a number between zero and one.",
                        )
                    })
            })
            .transpose()?;
        values.insert(entry.name.clone(), value.clone());
        confidence.insert(
            entry.name.clone(),
            if incomplete {
                Value::Null
            } else {
                json!(score)
            },
        );
    }
    let output =
        json!({"values": values, "confidence": confidence, "truncated": response.truncated});
    if output.to_string().len() > MAX_OUTPUT_BYTES {
        return Err(Failure::permanent(FailureCode::BudgetExceeded));
    }
    Ok(output)
}

pub fn output_schema(config: &Map<String, Value>) -> Value {
    let values = parse(config).ok().map(|config| {
        let properties = config.fields.into_iter().map(|entry| {
            let schema = match entry.kind {
                FieldType::Text => json!({"type":["string","null"]}),
                FieldType::Date => json!({"type":["string","null"],"format":"date"}),
                FieldType::Number => json!({"type":["number","null"]}),
                FieldType::Boolean => json!({"type":["boolean","null"]}),
                FieldType::Enum => {
                    let mut values: Vec<Value> = entry.choices.unwrap().into_iter().map(Value::String).collect();
                    values.push(Value::Null);
                    json!({"enum":values})
                }
            };
            (entry.name, schema)
        }).collect::<Map<_, _>>();
        let required: Vec<_> = properties.keys().collect();
        json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
    }).unwrap_or_else(|| json!({"type":"object"}));
    json!({"type":"object","properties":{
        "values":values,
        "confidence":{"type":"object","additionalProperties":{"type":["number","null"],"minimum":0,"maximum":1}},
        "truncated":{"type":"boolean"}
    },"required":["values","confidence","truncated"]})
}

pub fn manifest() -> NodeManifest {
    NodeManifest {
        kind: KIND.into(),
        version: 1,
        config_schema: json!({"type":"object","additionalProperties":false,"required":["fields"],"properties":{
            "inputPointer":{"type":"string","description":"JSON Pointer to source text. Empty selects the whole input."},
            "attachmentsPointer":{"type":"string","description":"JSON Pointer to an array of private file references."},
            "referenceDate":{"type":"string","format":"date"},
            "fields":{"type":"array","minItems":1,"items":{
                "type":"object","additionalProperties":false,"required":["name","type"],"properties":{
                    "name":{"type":"string","pattern":"^[A-Za-z][A-Za-z0-9_]*$"},
                    "type":{"enum":["text","number","date","boolean","enum"]},
                    "instructions":{"type":"string"},
                    "enum":{"type":"array","minItems":1,"uniqueItems":true,"items":{"type":"string","minLength":1}}
                },
                "if":{"properties":{"type":{"const":"enum"}}},"then":{"required":["enum"]},"else":{"not":{"required":["enum"]}}
            }},
            "simulationResponse":{"type":"object","required":["answers"],"properties":{
                "answers":{"type":"object","additionalProperties":{"type":"object","required":["value"],"properties":{
                    "value":{},"confidence":{"type":["number","null"],"minimum":0,"maximum":1}
                }}},
                "truncated":{"type":"boolean"}
            }}
        }}),
        output_ports: vec!["success".into(), "error".into()],
        has_external_side_effects: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn booking_task() -> Task {
        serde_json::from_value(json!({
            "id":"extract","idempotencyKey":"run/extract","attempt":1,"deadlineMs":30_000,
            "node":{"id":"extract","type":KIND,"label":"Booking","config":{
                "inputPointer":"/body","referenceDate":"2026-11-25",
                "fields":[
                    {"name":"guest_count","type":"number","instructions":"Expected guests"},
                    {"name":"enquiry","type":"enum","enum":["booking","other"]},
                    {"name":"event_date","type":"date"}
                ]
            }},
            "input":{"body":"Could we book the band for 80 guests?","headers":{"authorization":"sensitive"}}
        })).unwrap()
    }

    #[test]
    fn worker_request_only_contains_selected_text_and_configured_fields() {
        let task = booking_task();
        let prepared = prepare(&task).unwrap();
        assert_eq!(
            serde_json::to_value(prepared.request(&[])).unwrap(),
            json!({
                "model":"clairic","state":"Could we book the band for 80 guests?",
                "reference_date":"2026-11-25","fields":{
                    "guest_count":{"type":"number","instructions":"Expected guests"},
                    "enquiry":{"type":"enum","enum":["booking","other"]},
                    "event_date":{"type":"date"}
                }
            })
        );
    }

    #[test]
    fn accepts_media_without_text_and_rejects_unscoped_reference_shapes() {
        let mut task = booking_task();
        task.node.config.remove("inputPointer");
        task.node
            .config
            .insert("attachmentsPointer".into(), json!("/attachments"));
        task.input = json!({"attachments":[{
            "id":"1".repeat(64), "name":"booking.pdf", "contentType":"application/pdf", "size":10
        }]});
        let prepared = prepare(&task).unwrap();
        assert_eq!(prepared.files()[0].name, "booking.pdf");
        let media = [MediaPart {
            content_type: "application/pdf".into(),
            base64: "encoded".into(),
        }];
        let request = serde_json::to_value(prepared.request(&media)).unwrap();
        assert_eq!(request["state"], "");
        assert_eq!(request["documents"][0]["base64"], "encoded");
        assert!(request.get("images").is_none());

        task.input["attachments"][0]["id"] = json!("https://example.test/private.pdf");
        assert!(matches!(
            prepare(&task),
            Err(Failure {
                code: FailureCode::InvalidConfig,
                ..
            })
        ));
    }

    #[test]
    fn incomplete_and_unknown_answers_are_distinct_from_provider_contract_failures() {
        let task = booking_task();
        let prepared = prepare(&task).unwrap();
        let mut response = json!({"truncated":true,"answers":{
            "guest_count":{"value":80,"confidence":0.95},
            "enquiry":{"value":"booking","complete":false},
            "event_date":{"value":null,"guess":"2027-02-29"}
        },"state_text":"must not be stored"});
        assert_eq!(
            prepared.complete(response.clone()),
            Outcome::success(json!({
                "values":{"guest_count":80,"enquiry":null,"event_date":null},
                "confidence":{"guest_count":0.95,"enquiry":null,"event_date":null},
                "truncated":true
            }))
        );
        response["answers"]["event_date"]["value"] = json!("2027-02-29");
        assert!(
            matches!(prepared.complete(response), Outcome::Failed { failure } if failure.code == FailureCode::InvalidResponse && failure.detail.as_ref().unwrap().path == "/answers/event_date/value")
        );
        assert!(
            matches!(simulate(&task), Outcome::Failed { failure } if failure.code == FailureCode::InvalidConfig && failure.detail.as_ref().unwrap().path == "/simulationResponse")
        );
    }
}
