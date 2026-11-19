use connector_sdk::{FailureCode, Outcome, Task};
use connectors_validation::{execute, validate};
use serde_json::{Value, json};
fn task(schema: Value, input: Value) -> Task {
    serde_json::from_value(json!({"id":"task","idempotencyKey":"run:validate","node":{"id":"validate","type":"Validate Schema","label":"Validate","config":{"schema":schema}},"input":input,"attempt":1,"deadlineMs":1000})).unwrap()
}
fn output(schema: Value, input: Value) -> Value {
    let Outcome::Complete { output, .. } = execute(&task(schema, input)) else {
        panic!("Expected validation result")
    };
    output
}
#[test]
fn required_fields_types_ranges_enum_and_formats_report_without_choosing_a_branch() {
    let schema = json!({"type":"object","required":["eventId","email","amount"],"additionalProperties":false,"properties":{"eventId":{"type":"string","minLength":1},"email":{"type":"string","format":"email"},"amount":{"type":"number","minimum":1,"maximum":100},"state":{"enum":["open","closed"]}}});
    let good = json!({"eventId":"event-1","email":"person@example.com","amount":25});
    let result = output(schema.clone(), good.clone());
    assert_eq!(result["valid"], true);
    assert_eq!(result["value"], good);
    assert_eq!(result["errors"], json!([]));
    let invalid = output(
        schema,
        json!({"email":"not-email","amount":-1,"state":"unknown"}),
    );
    assert_eq!(invalid["valid"], false);
    let errors = invalid["errors"].as_array().unwrap();
    assert!(
        errors
            .iter()
            .any(|e| e["keyword"] == "required" && e["missingProperty"] == "eventId")
    );
    assert!(errors.iter().any(|e| e["keyword"] == "format"));
    assert!(errors.iter().any(|e| e["keyword"] == "minimum"));
    assert!(errors.iter().any(|e| e["keyword"] == "enum"));
}
#[test]
fn unsupported_or_external_schema_work_is_rejected_before_compilation() {
    for schema in [
        json!({"$ref":"https://private.example/schema"}),
        json!({"$ref":"file:///private"}),
        json!({"pattern":"(a+)+$"}),
        json!({"oneOf":[{},{}]}),
        json!({"format":"custom-typo"}),
        json!({"type":"not-a-type"}),
    ] {
        assert!(validate(json!({"schema":schema}).as_object().unwrap()).is_err());
    }
    let mut nested = json!({"type":"string"});
    for _ in 0..20 {
        nested = json!({"type":"array","items":nested})
    }
    assert!(validate(json!({"schema":nested}).as_object().unwrap()).is_err());
}
#[test]
fn diagnostic_budgets_do_not_turn_invalid_data_into_a_valid_result_or_echo_values() {
    let required: Vec<_> = (0..100).map(|i| format!("field{i}")).collect();
    let result = output(json!({"type":"object","required":required}), json!({}));
    assert_eq!(result["valid"], false);
    assert_eq!(result["errorsTruncated"], true);
    assert!(result["errors"].as_array().unwrap().len() <= 32);
    let secret = "private-value-must-not-appear-in-errors";
    let result = output(json!({"type":"number"}), json!(secret));
    assert!(!result["errors"].to_string().contains(secret));
    assert_eq!(result["value"], secret);
}
#[test]
fn editor_json_and_selected_input_are_supported_with_missing_input_errors() {
    let mut t = task(
        json!({"type":"integer"}).to_string().into(),
        json!({"payload":10}),
    );
    t.node.config.insert("pointer".into(), json!("/payload"));
    let Outcome::Complete { output, .. } = execute(&t) else {
        panic!()
    };
    assert_eq!(output["valid"], true);
    assert_eq!(output["value"], 10);
    t.node.config.insert("pointer".into(), json!("/absent"));
    let Outcome::Failed { failure } = execute(&t) else {
        panic!()
    };
    assert_eq!(failure.code, FailureCode::MissingInput);
}
#[test]
fn integer_bounds_do_not_round_large_identifiers() {
    let result = output(
        json!({"type":"integer","maximum":9007199254740992u64}),
        json!(9007199254740993u64),
    );
    assert_eq!(result["valid"], false);
}
