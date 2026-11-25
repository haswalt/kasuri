//! Runtime data discovery. Schemas are intentionally open where data depends on input,
//! selected branches, a child contract or user mappings; never invent a fixed shape.
use serde_json::{Map, Value, json};
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required})
}
fn decode(v: Option<&Value>) -> Value {
    match v {
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
        Some(v) => v.clone(),
        None => Value::Null,
    }
}
fn inferred(v: &Value, depth: usize) -> Value {
    if depth > 8 {
        return json!({});
    }
    match v {
        Value::Null => json!({"type":"null"}),
        Value::Bool(_) => json!({"type":"boolean"}),
        Value::Number(n) => json!({"type":if n.is_f64(){"number"}else{"integer"}}),
        Value::String(_) => json!({"type":"string"}),
        Value::Array(_) => json!({"type":"array"}),
        Value::Object(o) if o.len() == 1 && o.contains_key("$input") => json!({}),
        Value::Object(o) => object(
            json!(
                o.iter()
                    .take(100)
                    .map(|(k, v)| (k.clone(), inferred(v, depth + 1)))
                    .collect::<Map<_, _>>()
            ),
            &[],
        ),
    }
}
fn expression(v: &Value) -> Value {
    match v["op"].as_str().unwrap_or("") {
        "literal" => inferred(&v["value"], 0),
        "exists" | "equals" | "greater_than" | "less_than" | "and" | "or" | "not" => {
            json!({"type":"boolean"})
        }
        "lower" | "upper" | "trim" | "concat" | "to_text" | "date_iso" => json!({"type":"string"}),
        "length" | "date_parse" | "date_add_ms" => json!({"type":"integer"}),
        "add" | "subtract" | "multiply" | "divide" => json!({"type":"number"}),
        _ => json!({}),
    }
}
/// Generic description for kinds without a specific contract; every implemented
/// connector must have its own entry (enforced by tests).
pub const UNDESCRIBED: &str =
    "Inspect implemented connector configuration and tested public workflows.";
pub fn describe(kind: &str, config: &Map<String, Value>) -> Value {
    let mut input = json!({});
    let mut output = json!({});
    let mut selection = Value::Null;
    let mut child = Value::Null;
    let description = match kind {
        "Execute Workflow Trigger" => {
            // The declared contracts are enforced at invocation: inputSchema on the
            // child's input (passed onward unchanged) and outputSchema on its result.
            let declared = |key: &str| match decode(config.get(key)) {
                Value::Null => json!({}),
                schema => schema,
            };
            input = declared("inputSchema");
            output = input.clone();
            child = json!({"inputSchema":input,"outputSchema":declared("outputSchema"),"outputNodeId":config.get("outputNodeId").cloned().unwrap_or(Value::Null)});
            "Starts a child run with one JSON value validated against config.inputSchema and passes it onward unchanged. The node at outputNodeId returns the child result, validated against config.outputSchema; see childContract."
        }
        "Manual Trigger" | "Webhook" | "Schedule" => {
            "Starts a run with one JSON value. Trigger input is passed onward unchanged; child and webhook parsing contracts are explicit."
        }
        "Map / Transform" => {
            let mapping = decode(config.get("mapping"));
            if let Some(m) = mapping.as_object() {
                output = object(
                    json!(
                        m.iter()
                            .map(|(k, v)| (k.clone(), inferred(v, 0)))
                            .collect::<Map<_, _>>()
                    ),
                    &[],
                );
            }
            "Builds one object from literal values and explicit $input JSON Pointer references. Literal strings are never expressions."
        }
        "Expressions" => {
            if let Some(fields) = config.get("fields").and_then(Value::as_object) {
                output = object(
                    json!(
                        fields
                            .iter()
                            .map(|(k, v)| (k.clone(), expression(v)))
                            .collect::<Map<_, _>>()
                    ),
                    &[],
                );
            }
            "Builds a typed object using bounded operations and approved graph-ancestor references. Missing paths fail unless explicitly handled."
        }
        "HTTP Request" => {
            output = object(
                json!({"status":{"type":"integer","minimum":100,"maximum":599},"contentType":{"type":["string","null"]},"body":if config.get("responseType").and_then(Value::as_str)==Some("text"){json!({"type":"string"})}else{json!({})}}),
                &["status", "contentType", "body"],
            );
            "Dispatches a bounded HTTPS request. Result body is JSON or text, inside /body; status is /status. Simulation uses an explicit fixture."
        }
        connectors_email::TRIGGER => {
            output = connectors_email::inbound_schema();
            "Starts a published workflow from a configured Cloudflare Email route. Outputs the SMTP envelope, subject, plain-text body, message ID, automatic-message indicator and attachment references."
        }
        connectors_email::SEND => {
            input = connectors_email::send_input_schema();
            output = object(
                json!({"status":{"enum":["accepted","preview"]},"messageId":{"type":["string","null"]}}),
                &["status", "messageId"],
            );
            "Sends one plain-text email via Cloudflare Email Sending. Literal configuration overrides corresponding input fields. Simulations validate the message and return preview without delivery."
        }
        connectors_clairic::KIND => {
            selection = json!({"pointer":config.get("inputPointer").and_then(Value::as_str).unwrap_or(""),"selectedSchema":{"type":"string","minLength":1}});
            if let Some(pointer) = config.get("attachmentsPointer").and_then(Value::as_str) {
                selection["attachmentsPointer"] = json!(pointer);
            } else if selection["pointer"] == "" {
                input = json!({"type":"string","minLength":1});
            }
            output = connectors_clairic::output_schema(config);
            "Extracts configured typed values from selected text and private image or PDF references with Cloudflare Workers AI. Read /values and /confidence by field name; null means there is no usable answer or score. Simulations require a fixture and never call the model."
        }
        "Parse Body" => {
            output = object(
                json!({"valid":{"type":"boolean"},"value":{},"errors":{"type":"array"}}),
                &["valid", "value", "errors"],
            );
            "Decodes JSON/XML/text/form into /value and returns /valid plus diagnostics. Invalid content is a routable result, not a transport exception."
        }
        "Validate Schema" => {
            output = object(
                json!({"valid":{"type":"boolean"},"value":{},"errors":{"type":"array"},"errorsTruncated":{"type":"boolean"}}),
                &["valid", "value", "errors", "errorsTruncated"],
            );
            "Validates selected data against an offline JSON Schema profile. Outputs validity and diagnostics; use IF to choose the response or next action."
        }
        "IF / Condition" => {
            "Tests typed conditions and passes the original input along exactly one true or false output. The error output is separate from false."
        }
        "Switch / Router" => {
            "Checks named routes in order, passing input through the first match or fallback. Output names come from config.rules[].port, plus fallback/error."
        }
        "Filter" | "Sort" | "Aggregate" | "Split / Batch" | "Each / Loop" => {
            let pointer = config
                .get("arrayPointer")
                .and_then(Value::as_str)
                .unwrap_or("");
            let mut array = json!({"type":"array"});
            if matches!(kind, "Each / Loop" | "Split / Batch" | "Aggregate") {
                array["maxItems"] = json!(if kind == "Each / Loop" { 100 } else { 1000 });
            }
            if pointer.is_empty() {
                input = array.clone();
            }
            selection = json!({"pointer":pointer,"schema":array});
            match kind {
                "Split / Batch" => {
                    output = object(
                        json!({"batches":{"type":"array","items":{"type":"array"}},"total":{"type":"integer"},"batchSize":{"type":"integer"}}),
                        &["batches", "total", "batchSize"],
                    );
                    "Splits the selected array into explicit ordered batches. It does not fan out automatically; use Each to process them."
                }
                "Aggregate" => {
                    output = match config.get("operation").and_then(Value::as_str) {
                        Some("count") => json!({"type":"integer","minimum":0}),
                        Some("sum") => json!({"type":"number"}),
                        Some("collect") => json!({"type":"array"}),
                        _ => json!({}),
                    };
                    "Consumes a selected array and counts, sums or collects selected item values. Output is the number or array itself, not a named wrapper. Item pointer selects /output/... after Each."
                }
                "Each / Loop" => {
                    output = object(
                        json!({"items":{"type":"array","items":{"type":"object","properties":{"index":{"type":"integer"},"itemId":{"type":"string"},"runId":{"type":["string","null"]},"status":{"enum":["pending","running","succeeded","failed","cancelled"]},"output":{},"error":{"type":"string"}}}},"total":{"type":"integer"},"succeeded":{"type":"integer"},"failed":{"type":"integer"}}),
                        &["items", "total", "succeeded", "failed"],
                    );
                    "Runs a pinned published child per original item. /items preserves input order with output or error per item; child-node retries do not repeat successful siblings."
                }
                "Filter" => {
                    output = json!({"type":"array"});
                    "Filters a selected array using typed conditions against each item. Output is an array, including an empty array when nothing matches."
                }
                _ => {
                    output = json!({"type":"array"});
                    "Sorts a selected array by a consistent numeric or string field. Output is the sorted array; missing or mixed keys fail."
                }
            }
        }
        "Execute Workflow" => {
            let background = config.get("mode").and_then(Value::as_str) == Some("background");
            output = object(
                json!({"runId":{"type":"string"},"workflowId":{"type":"string"},"revision":{"type":"integer"},"status":{"type":"string"},"output":{}}),
                if background {
                    &["runId", "workflowId", "revision", "status"]
                } else {
                    &["runId", "workflowId", "revision", "status", "output"]
                },
            );
            "Invokes an internal published child. Input is one selected JSON value; awaited results are wrapped at /output. Inspect the child's Execute Workflow Trigger output contract; background mode returns only a child reference."
        }
        "Join" => {
            input = json!({"type":"object"});
            output = json!({"type":"object"});
            "Waits for all incoming branches. Keys are source node IDs; collect policy wraps each branch as status/output/error, while all uses raw successful outputs."
        }
        "Merge" => {
            if config.contains_key("operation") {
                input = json!({"type":"object"});
            }
            if matches!(
                config.get("operation").and_then(Value::as_str),
                Some("append" | "by_key")
            ) {
                output = json!({"type":"array"});
            }
            "Combines settled branch inputs. Explicit policies consume an object keyed by source IDs; append/by_key return arrays and select returns the selected source unchanged. Empty config preserves legacy pass-through/keyed behavior."
        }
        "JSON Utilities" => {
            match config.get("operation").and_then(Value::as_str) {
                Some("stringify") => output = json!({"type":"string"}),
                Some("parse") => input = json!({"type":"string"}),
                // Missing or unknown operations fail at runtime; promise no contract.
                _ => {}
            }
            "Parses JSON text to a value (operation parse) or stringifies a value to JSON text (operation stringify). config.operation is required; parse failures use the error port."
        }
        "Request Context" => {
            output = match config.get("part").and_then(Value::as_str) {
                Some("raw_body") => json!({"type":"string"}),
                Some("headers" | "query") => json!({"type":"object"}),
                Some("body") => json!({}),
                _ => json!({"type":"object"}),
            };
            "Reads immutable original trigger data or approved request metadata. Consumed authentication credentials and private host context are excluded."
        }
        "Context Store" | "Data Tables" => {
            output = json!({"type":"object","properties":{"status":{"type":"string"},"row":{"type":["object","null"]},"rows":{"type":"array"},"table":{"type":"object"},"inserted":{"type":"boolean"},"dataRevision":{"type":"integer"}}});
            "Applies an explicitly selected dataset command. Results depend on operation: keyed reads/writes use row.value, insert-if-absent returns inserted, and queries return rows with a continuation cursor. Data scopes remain mandatory."
        }
        "Respond to Webhook" => {
            "Prepares a JSON/text HTTP status, headers and body at this position, then passes input onward unchanged. Sending a response does not cancel subsequent background work; a second response is an error."
        }
        "Wait / Delay" => {
            "Persists a durable delay and then passes input unchanged. No thread is held asleep; pause/cancel/restart follow the run checkpoint."
        }
        "Split / Parallel" => {
            "Enables bounded durable branch concurrency and passes input to each connected success branch. Use Join for explicit completion/failure semantics."
        }
        "Error Handler" => {
            "Receives the explicit failed-node error output and passes it onward. It does not independently intercept unrelated node failures."
        }
        _ => UNDESCRIBED,
    };
    let mut contract = json!({"description":description,"inputSchema":input,"outputSchema":output,"selectedInput":selection,"schemaSemantics":"Schemas describe successful runtime results and may be open where user data or selected contracts determine the shape. Failure routing is separate. inputSchema describes the value the connector receives; with format 2 inputBindings that value is the assembled object, not the parent output. No sample values or secrets are inferred from workspace data.","documentationUrl":"https://github.com/haswalt/kasuri","documentationPath":"docs/connectors.md"});
    if !child.is_null() {
        contract["childContract"] = child;
    }
    contract
}
