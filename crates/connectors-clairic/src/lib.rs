//! Typed Clairic requests and results shared by the Worker and offline simulations.
use chrono::NaiveDate;
use connector_sdk::{Failure, FailureCode, NodeManifest, Outcome, Task};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};

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
}

fn parse(config: &Map<String, Value>) -> Result<Config, FailureCode> {
    let parsed: Config = serde_json::from_value(Value::Object(config.clone()))
        .map_err(|_| FailureCode::InvalidConfig)?;
    let mut names = HashSet::new();
    let valid = !parsed.fields.is_empty()
        && parsed.fields.iter().all(|entry| {
            let mut bytes = entry.name.bytes();
            let valid_name = bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
                && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_');
            let valid_choices = match (entry.kind, &entry.choices) {
                (FieldType::Enum, Some(choices)) => {
                    let mut unique = HashSet::new();
                    !choices.is_empty()
                        && choices
                            .iter()
                            .all(|value| !value.trim().is_empty() && unique.insert(value))
                }
                (FieldType::Enum, None) | (_, Some(_)) => false,
                (_, None) => true,
            };
            valid_name && names.insert(&entry.name) && valid_choices
        })
        && parsed.input_pointer.as_ref().is_none_or(|pointer| {
            pointer.is_empty()
                || (pointer.starts_with('/')
                    && pointer
                        .split('~')
                        .skip(1)
                        .all(|part| part.starts_with('0') || part.starts_with('1')))
        });
    if !valid {
        return Err(FailureCode::InvalidConfig);
    }
    Ok(parsed)
}

pub fn validate(config: &Map<String, Value>) -> Result<(), FailureCode> {
    let config = parse(config)?;
    if let Some(fixture) = &config.simulation_response {
        normalize(&config, fixture).map_err(|_| FailureCode::InvalidConfig)?;
    }
    Ok(())
}

pub struct Prepared<'a> {
    config: Config,
    text: &'a str,
}

impl Prepared<'_> {
    pub fn request(&self) -> Request<'_> {
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
        }
    }

    pub fn complete(&self, response: Value) -> Outcome {
        serde_json::from_value::<ModelResponse>(response)
            .map_err(|_| FailureCode::InvalidResponse)
            .and_then(|response| normalize(&self.config, &response))
            .map_or_else(failed, Outcome::success)
    }
}

pub fn prepare(task: &Task) -> Result<Prepared<'_>, FailureCode> {
    let config = parse(&task.node.config)?;
    let text = task
        .input
        .pointer(config.input_pointer.as_deref().unwrap_or(""))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or(FailureCode::MissingInput)?;
    Ok(Prepared { config, text })
}

pub fn simulate(task: &Task) -> Outcome {
    prepare(task)
        .and_then(|prepared| {
            let fixture = prepared
                .config
                .simulation_response
                .as_ref()
                .ok_or(FailureCode::InvalidConfig)?;
            normalize(&prepared.config, fixture)
        })
        .map_or_else(failed, Outcome::success)
}

pub fn failed(code: FailureCode) -> Outcome {
    Outcome::Failed {
        failure: Failure::permanent(code),
    }
}

fn normalize(config: &Config, response: &ModelResponse) -> Result<Value, FailureCode> {
    let mut values = Map::new();
    let mut confidence = Map::new();
    for entry in &config.fields {
        let answer = response
            .answers
            .get(&entry.name)
            .and_then(Value::as_object)
            .ok_or(FailureCode::InvalidResponse)?;
        let incomplete = answer.get("valid").and_then(Value::as_bool) == Some(false)
            || answer.get("complete").and_then(Value::as_bool) == Some(false);
        let value = if incomplete {
            &Value::Null
        } else {
            answer.get("value").ok_or(FailureCode::InvalidResponse)?
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
            return Err(FailureCode::InvalidResponse);
        }
        let score = answer
            .get("confidence")
            .filter(|value| !value.is_null())
            .map(|value| {
                value
                    .as_f64()
                    .filter(|score| (0.0..=1.0).contains(score))
                    .ok_or(FailureCode::InvalidResponse)
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
        return Err(FailureCode::BudgetExceeded);
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
            "inputPointer":{"type":"string","description":"JSON Pointer to the source text. Empty selects the whole input."},
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
            serde_json::to_value(prepared.request()).unwrap(),
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
        assert_eq!(
            prepared.complete(response),
            failed(FailureCode::InvalidResponse)
        );
        assert_eq!(simulate(&task), failed(FailureCode::InvalidConfig));
    }
}
