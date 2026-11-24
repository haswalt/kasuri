use serde_json::{Map, Value, json};
use workflow_expressions::{validate, validate_fields};

fn fields(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}
fn same(fields: Map<String, Value>) {
    let config = fields_config(&fields);
    assert_eq!(validate(&config), validate_fields(&fields), "{fields:?}");
}
fn fields_config(fields: &Map<String, Value>) -> Map<String, Value> {
    json!({"fields":fields}).as_object().unwrap().clone()
}

#[test]
fn bindings_validation_matches_expressions_config_validation() {
    same(fields(json!({"a":{"op":"node","nodeId":"x","path":"/v"}})));
    same(Map::new());
    same(fields(json!({"a":{"op":"unknown"}})));
    same(fields(json!({"":{"op":"literal","value":1}})));
    // Probe either side of the byte budget so both entry points share one limit.
    let overhead = serde_json::to_vec(&json!({"fields":{"a":{"op":"literal","value":""}}}))
        .unwrap()
        .len();
    for padding in [
        65_536 - overhead - 1,
        65_536 - overhead,
        65_536 - overhead + 1,
    ] {
        let value = "x".repeat(padding);
        same(fields(json!({"a":{"op":"literal","value":value}})));
    }
    let at_limit = "x".repeat(65_536 - overhead);
    assert!(validate_fields(&fields(json!({"a":{"op":"literal","value":at_limit}}))).is_ok());
}
