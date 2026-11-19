use connector_sdk::FailureCode;
use connectors_data::{access, prepare};
use serde_json::{Value, json};
use workflow_data::MemoryStore;
use workflow_schema::Node;
fn node(kind: &str, command: Value) -> Node {
    serde_json::from_value(
        json!({"id":"data", "type":kind,"label":"Data", "config":{"command":command}}),
    )
    .unwrap()
}
#[test]
fn static_access_and_typed_input_resolution_share_data_semantics() {
    let mut store = MemoryStore::default();
    let create = node(
        "Context Store",
        json!({"operation":"create_table","definition":{"id":"cache","name":"Cache","kind":"context"}}),
    );
    let prepared = prepare(&create, &json!({})).unwrap();
    assert!(prepared.access.writes);
    store.execute(prepared.command, Some("create"), 0).unwrap();
    let claim = node(
        "Context Store",
        json!({"operation":"insert_if_absent","tableId":"cache","key":{"$input":"/event/id"},"value":{"payload":{"$input":"/body"}},"ttlMs":{"$input":"/ttl"}}),
    );
    let prepared = prepare(
        &claim,
        &json!({"event":{"id":"event-1"},"body":[1,true],"ttl":100}),
    )
    .unwrap();
    assert_eq!(prepared.access.table_id, "cache");
    let result = store
        .execute(prepared.command.clone(), Some("run:node"), 10)
        .unwrap();
    assert_eq!(result["inserted"], true);
    assert_eq!(result["row"]["value"]["payload"], json!([1, true]));
    assert_eq!(
        store
            .execute(prepared.command, Some("run:node"), 200)
            .unwrap(),
        result
    );
    let get = node(
        "Context Store",
        json!({"operation":"get","tableId":"cache","key":"event-1"}),
    );
    assert!(!access(&get).unwrap().writes);
    assert_eq!(
        store
            .execute(prepare(&get, &json!({})).unwrap().command, None, 110)
            .unwrap()["status"],
        "not_found"
    );
}
#[test]
fn routing_permissions_and_host_receipts_cannot_be_selected_by_input() {
    for command in [
        json!({"operation":{"$input":"/op"},"tableId":"cache","key":"x"}),
        json!({"operation":"get","tableId":{"$input":"/table"},"key":"x"}),
        json!({"operation":"get","tableId":"../private","key":"x"}),
        json!({"operation":"create_table","definition":{"id":"cache","name":"Cache","kind":"table"}}),
    ] {
        assert!(access(&node("Context Store", command)).is_err());
    }
    let unknown = node(
        "Context Store",
        json!({"operation":"put","tableId":"cache","key":"x","value":1,"scope":"live"}),
    );
    assert_eq!(
        prepare(&unknown, &json!({})).unwrap_err(),
        FailureCode::InvalidConfig
    );
    let missing = node(
        "Data Tables",
        json!({"operation":"get","tableId":"rows","key":{"$input":"/missing"}}),
    );
    assert_eq!(
        prepare(&missing, &json!({})).unwrap_err(),
        FailureCode::MissingInput
    );
    let malformed = node(
        "Data Tables",
        json!({"operation":"get","tableId":"rows","key":{"$input":"/id","fallback":"x"}}),
    );
    assert_eq!(
        prepare(&malformed, &json!({"id":"a"})).unwrap_err(),
        FailureCode::InvalidConfig
    );
}

#[test]
fn repeated_input_references_are_bounded_and_resolved_values_are_not_evaluated() {
    let references: Vec<_> = (0..100).map(|_| json!({"$input":"/payload"})).collect();
    let oversized = node(
        "Context Store",
        json!({"operation":"put","tableId":"cache","key":"x","value":references}),
    );
    assert_eq!(
        prepare(&oversized, &json!({"payload":"x".repeat(1024)})).unwrap_err(),
        FailureCode::BudgetExceeded
    );
    let literal = node(
        "Context Store",
        json!({"operation":"put","tableId":"cache","key":"x","value":{"$input":"/payload"}}),
    );
    let prepared = prepare(
        &literal,
        &json!({"payload":{"$input":"/private"},"private":"do not evaluate"}),
    )
    .unwrap();
    let serialized = serde_json::to_value(prepared.command).unwrap();
    assert_eq!(serialized["value"], json!({"$input":"/private"}));
}

#[test]
fn editor_json_text_and_dataset_kind_are_enforced_before_execution() {
    let mut store = MemoryStore::default();
    let mut create = node(
        "Context Store",
        json!({"operation":"create_table","definition":{"id":"cache","name":"Cache","kind":"context"}}),
    );
    create.config.insert(
        "command".into(),
        Value::String(create.config["command"].to_string()),
    );
    let prepared = prepare(&create, &json!({})).unwrap();
    connectors_data::apply(&mut store, prepared, "create", 0).unwrap();
    let wrong = node(
        "Data Tables",
        json!({"operation":"put","tableId":"cache","key":"x","value":{"x":1}}),
    );
    assert!(
        connectors_data::apply(
            &mut store,
            prepare(&wrong, &json!({})).unwrap(),
            "wrong-kind",
            0
        )
        .is_err()
    );
    assert!(store.rows.is_empty());
    assert!(!store.receipts.contains_key("wrong-kind"));
}
