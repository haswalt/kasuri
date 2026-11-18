use connector_sdk::{Connector, Failure, FailureCode, Outcome, Task};
use connectors_core::CoreConnector;
use serde_json::{Value, json};
use workflow_schema::Node;
fn execute(kind: &str, config: Value, input: Value) -> Outcome {
    let node: Node =
        serde_json::from_value(json!({"id":"test","type":kind,"label":"Test","config":config}))
            .unwrap();
    CoreConnector.execute(&Task {
        id: "task".into(),
        idempotency_key: "effect".into(),
        node,
        input,
        attempt: 1,
        deadline_ms: 30_000,
    })
}
#[test]
fn maps_literals_and_escaped_json_pointer_references() {
    assert_eq!(
        execute(
            "Map / Transform",
            json!({"mapping":{"name":{"$input":"/customer/name"},"escaped":{"$input":"/a~1b"},"literal":"hello","nested":{"key":true}}}),
            json!({"customer":{"name":"Ada"},"a/b":42})
        ),
        Outcome::success(
            json!({"name":"Ada","escaped":42,"literal":"hello","nested":{"key":true}})
        )
    );
}
#[test]
fn missing_values_are_distinct_from_explicit_null() {
    assert_eq!(
        execute(
            "IF / Condition",
            json!({"pointer":"/x","equals":null}),
            json!({"x":null})
        ),
        Outcome::Complete {
            output: json!({"x":null}),
            port: "true".into()
        }
    );
    assert_eq!(
        execute(
            "IF / Condition",
            json!({"pointer":"/x","equals":null}),
            json!({})
        ),
        Outcome::Failed {
            failure: Failure::permanent(FailureCode::MissingInput)
        }
    );
}
#[test]
fn invalid_configuration_fails_without_exposing_input() {
    assert_eq!(
        execute(
            "Map / Transform",
            json!({"mapping":"not JSON"}),
            json!({"token":"secret"})
        ),
        Outcome::Failed {
            failure: Failure::permanent(FailureCode::InvalidConfig)
        }
    );
    assert_eq!(
        execute("Wait / Delay", json!({"delayMs":-1}), Value::Null),
        Outcome::Failed {
            failure: Failure::permanent(FailureCode::InvalidConfig)
        }
    );
}
#[test]
fn json_parse_and_stringify_roundtrip() {
    let value = json!({"name":"Ada","items":[1,2]});
    let Outcome::Complete { output, .. } = execute(
        "JSON Utilities",
        json!({"operation":"stringify"}),
        value.clone(),
    ) else {
        panic!("stringify failed")
    };
    assert_eq!(
        execute("JSON Utilities", json!({"operation":"parse"}), output),
        Outcome::success(value)
    );
}
#[test]
fn wait_is_a_nonblocking_effect_and_accepts_ui_string_config() {
    assert_eq!(
        execute("Wait / Delay", json!({"delayMs":"2500"}), json!({"x":1})),
        Outcome::Wait {
            delay_ms: 2500,
            output: json!({"x":1})
        }
    );
}
#[test]
fn unsupported_nodes_never_execute_code_or_perform_http() {
    for kind in ["Code", "HTTP Request", "Slack"] {
        assert_eq!(
            execute(kind, json!({}), Value::Null),
            Outcome::Failed {
                failure: Failure::permanent(FailureCode::UnsupportedNode)
            }
        );
    }
}

#[test]
fn manifests_only_advertise_implemented_versioned_nodes() {
    let manifests = connectors_core::manifests();
    assert_eq!(manifests.len(), 9);
    assert!(
        manifests
            .iter()
            .all(|m| m.version == 1 && !m.has_external_side_effects)
    );
    assert!(
        !manifests
            .iter()
            .any(|m| m.kind == "Code" || m.kind == "HTTP Request")
    );
    assert_eq!(
        manifests
            .iter()
            .find(|m| m.kind == "IF / Condition")
            .unwrap()
            .output_ports,
        ["true", "false", "error"]
    );
}
