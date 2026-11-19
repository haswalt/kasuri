use serde_json::{Value, json};
use workflow_data::transfer::{ImportRow, encode, parse};
use workflow_data::{Command, ErrorCode, MemoryStore};
fn command(value: Value) -> Command {
    serde_json::from_value(value).unwrap()
}
fn store() -> MemoryStore {
    let mut store = MemoryStore::default();
    store.execute(command(json!({"operation":"create_table","definition":{"id":"events","name":"Events","kind":"context"}})),Some("create"),0).unwrap();
    store
}
#[test]
fn json_and_csv_round_trip_values_keys_expiry_and_large_integers() {
    let rows = vec![
        ImportRow {
            key: "=SUM(1,2)".into(),
            value: json!({"quoted":"a,\"b\"\nc","large":9007199254740993u64,"null":null,"list":[false,1]}),
            expires_at: Some(10000),
        },
        ImportRow {
            key: "unicode-✓".into(),
            value: json!("plain text"),
            expires_at: None,
        },
    ];
    for format in ["json", "csv"] {
        let encoded = encode(format, &rows).unwrap();
        assert_eq!(parse(format, &encoded).unwrap(), rows);
        assert!(encoded.contains("9007199254740993"));
    }
    assert_eq!(
        parse(
            "json",
            &format!(
                "{{\"schemaVersion\":1,\"tableId\":\"source\",\"rows\":{}}}",
                encode("json", &rows).unwrap()
            )
        )
        .unwrap(),
        rows
    );
    assert!(parse("csv", "key,value,expiresAt\nnot-json,{},\n").is_err());
    assert!(parse("json", "{\"schemaVersion\":2,\"rows\":[]}").is_err());
    assert!(parse("csv", "wrong,headers\n1,2").is_err());
    assert!(parse("json", &"x".repeat(131073)).is_err());
    assert!(parse("json", "[]").unwrap().is_empty());
}
#[test]
fn import_is_atomic_receipted_and_does_not_resurrect_expired_rows() {
    let mut store = store();
    let import = command(
        json!({"operation":"import_rows","tableId":"events","rows":[{"key":"new","value":{"count":1}},{"key":"expired","value":2,"expiresAt":5}],"expectedDataRevision":0}),
    );
    let first = store
        .execute(import.clone(), Some("import-once"), 10)
        .unwrap();
    assert_eq!(first["imported"], 1);
    assert_eq!(first["skippedExpired"], 1);
    assert_eq!(
        store.execute(import, Some("import-once"), 100).unwrap(),
        first
    );
    let before = store.rows.clone();
    let conflict = command(
        json!({"operation":"import_rows","tableId":"events","rows":[{"key":"other","value":1},{"key":"new","value":2}]}),
    );
    assert_eq!(
        store
            .execute(conflict, Some("conflict"), 10)
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    assert_eq!(store.rows, before);
    let stale = command(
        json!({"operation":"import_rows","tableId":"events","mode":"upsert","expectedDataRevision":0,"rows":[{"key":"new","value":99}]}),
    );
    assert!(store.execute(stale, Some("stale"), 10).is_err());
    assert_eq!(store.rows, before);
    let duplicate = command(
        json!({"operation":"import_rows","tableId":"events","rows":[{"key":"dup","value":1},{"key":"dup","value":2}]}),
    );
    assert!(store.execute(duplicate, Some("duplicate"), 10).is_err());
    assert_eq!(store.rows, before);
    let replace = command(
        json!({"operation":"import_rows","tableId":"events","mode":"upsert","expectedDataRevision":1,"rows":[{"key":"new","value":99}]}),
    );
    assert_eq!(
        store.execute(replace, Some("replace"), 10).unwrap()["imported"],
        1
    );
}
#[test]
fn counters_preserve_ttl_and_fields_and_reject_overflow_without_writes() {
    let mut store = store();
    store.execute(command(json!({"operation":"put","tableId":"events","key":"counter","value":{"count":9007199254740993u64,"other":true},"ttlMs":1000})),Some("initial"),10).unwrap();
    let increment = command(
        json!({"operation":"increment","tableId":"events","key":"counter","field":"count","amount":2}),
    );
    let first = store
        .execute(increment.clone(), Some("count-once"), 20)
        .unwrap();
    assert_eq!(first["row"]["value"]["count"], 9007199254740995u64);
    assert_eq!(first["row"]["value"]["other"], true);
    assert_eq!(first["row"]["expiresAt"], 1010);
    assert_eq!(
        store.execute(increment, Some("count-once"), 30).unwrap(),
        first
    );
    store.execute(command(json!({"operation":"put","tableId":"events","key":"max","value":{"count":i64::MAX}})),Some("max"),30).unwrap();
    let before = store.rows.clone();
    assert!(store.execute(command(json!({"operation":"increment","tableId":"events","key":"max","field":"count","amount":1})),Some("overflow"),30).is_err());
    assert_eq!(store.rows, before);
    let created=store.execute(command(json!({"operation":"increment","tableId":"events","key":"fresh","field":"count","amount":1,"initial":10})),Some("fresh"),30).unwrap();
    assert_eq!(created["row"]["value"]["count"], 11);
}

#[test]
fn published_transfer_files_are_equivalent_and_importable() {
    let json = parse(
        "json",
        include_str!("../../../examples/datasets/records.json"),
    )
    .unwrap();
    let csv = parse(
        "csv",
        include_str!("../../../examples/datasets/records.csv"),
    )
    .unwrap();
    assert_eq!(json, csv);
    let mut store = store();
    let imported = store
        .execute(
            Command::ImportRows {
                table_id: "events".into(),
                rows: json,
                mode: Default::default(),
                expected_data_revision: Some(0),
            },
            Some("public-samples"),
            0,
        )
        .unwrap();
    assert_eq!(imported["imported"], 2);
}
