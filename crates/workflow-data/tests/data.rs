use serde_json::{Value, json};
use workflow_data::{Command, ErrorCode, Filter, MemoryStore, Operator, Query};
fn command(value: Value) -> Command {
    serde_json::from_value(value).unwrap()
}
fn create(store: &mut MemoryStore) {
    store.execute(command(json!({"operation":"create_table","definition":{"id":"events","name":"Events","kind":"table","fields":[{"name":"active","kind":"boolean","required":true}]}})),Some("create-events"),0).unwrap();
}
fn insert(key: &str, value: Value) -> Command {
    command(json!({"operation":"insert_if_absent","tableId":"events","key":key,"value":value}))
}
#[test]
fn atomic_claims_and_receipts_replay_original_result_without_repeating_a_write() {
    let mut store = MemoryStore::default();
    create(&mut store);
    let action = insert("event-1", json!({"active":true}));
    let first = store
        .execute(action.clone(), Some("run-a:claim"), 10)
        .unwrap();
    assert_eq!(first["inserted"], true);
    assert_eq!(
        store
            .execute(action.clone(), Some("run-a:claim"), 999)
            .unwrap(),
        first
    );
    assert_eq!(store.tables["events"].data_revision, 1);
    let competing = store.execute(action, Some("run-b:claim"), 10).unwrap();
    assert_eq!(competing["inserted"], false);
    assert_eq!(competing["status"], "existing");
    assert_eq!(
        store
            .execute(
                insert("different", json!({"active":true})),
                Some("run-a:claim"),
                10
            )
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    assert_eq!(store.rows.len(), 1);
}
#[test]
fn ttl_reclaims_are_explicit_new_effects_and_versions_prevent_delete_reinsert_aba() {
    let mut store = MemoryStore::default();
    create(&mut store);
    let action = command(
        json!({"operation":"insert_if_absent","tableId":"events","key":"event","value":{"active":true},"ttlMs":10}),
    );
    let first = store
        .execute(action.clone(), Some("first-claim"), 100)
        .unwrap();
    let old_version = first["row"]["version"].clone();
    assert_eq!(
        store
            .execute(
                command(json!({"operation":"get","tableId":"events","key":"event"})),
                None,
                110
            )
            .unwrap()["status"],
        "not_found"
    );
    assert_eq!(
        store
            .execute(action.clone(), Some("first-claim"), 200)
            .unwrap(),
        first,
        "Recovery never repeats an expired original claim"
    );
    let reclaimed = store.execute(action, Some("new-claim"), 200).unwrap();
    assert_eq!(reclaimed["inserted"], true);
    assert_ne!(reclaimed["row"]["version"], old_version);
    let stale=store.execute(command(json!({"operation":"put","tableId":"events","key":"event","value":{"active":false},"expectedVersion":old_version})),Some("stale-cas"),201).unwrap();
    assert_eq!(stale["status"], "conflict");
    let current = reclaimed["row"]["version"].clone();
    store.execute(command(json!({"operation":"delete","tableId":"events","key":"event","expectedVersion":current})),Some("delete-current"),202).unwrap();
    let recreated = store
        .execute(
            insert("event", json!({"active":true})),
            Some("recreate"),
            203,
        )
        .unwrap();
    assert!(recreated["row"]["version"].as_u64() > current.as_u64());
    assert_eq!(store.execute(command(json!({"operation":"put","tableId":"events","key":"event","value":{"active":false},"expectedVersion":current})),Some("aba-cas"),204).unwrap()["status"],"conflict");
}
#[test]
fn failed_batches_roll_back_rows_metadata_and_receipts() {
    let mut store = MemoryStore::default();
    create(&mut store);
    let before = store.clone();
    let result = store.execute_batch(
        vec![
            insert("first", json!({"active":true})),
            insert("second", json!({"active":"wrong type"})),
        ],
        "batch",
        0,
    );
    assert_eq!(result.unwrap_err().code, ErrorCode::InvalidRequest);
    assert_eq!(store.rows, before.rows);
    assert_eq!(store.tables, before.tables);
    assert!(!store.receipts.contains_key("batch"));
    let commands = vec![
        insert("first", json!({"active":true})),
        insert("second", json!({"active":false})),
    ];
    let result = store.execute_batch(commands.clone(), "batch", 0).unwrap();
    assert_eq!(store.execute_batch(commands, "batch", 100).unwrap(), result);
    assert_eq!(store.tables["events"].data_revision, 2);
}
#[test]
fn sparse_queries_advance_bounded_cursors_without_skipping_matching_rows() {
    let mut store = MemoryStore::default();
    create(&mut store);
    for i in 0..1005 {
        store
            .execute(
                insert(&format!("event-{i:04}"), json!({"active":i>=1000})),
                Some(&format!("insert-{i}")),
                0,
            )
            .unwrap();
    }
    let query = Query {
        cursor: None,
        limit: 2,
        filters: vec![Filter {
            pointer: "/active".into(),
            operator: Operator::Eq,
            value: json!(true),
        }],
    };
    let first = store
        .execute(
            Command::Query {
                table_id: "events".into(),
                query: query.clone(),
            },
            None,
            0,
        )
        .unwrap();
    assert_eq!(first["rows"], json!([]));
    assert_eq!(first["scanned"], 1000);
    assert_eq!(first["nextCursor"], "event-0999");
    let mut next = query.clone();
    next.cursor = first["nextCursor"].as_str().map(str::to_owned);
    let second = store
        .execute(
            Command::Query {
                table_id: "events".into(),
                query: next,
            },
            None,
            0,
        )
        .unwrap();
    assert_eq!(second["rows"][0]["key"], "event-1000");
    assert_eq!(second["rows"][1]["key"], "event-1001");
    let mut next = query;
    next.cursor = second["nextCursor"].as_str().map(str::to_owned);
    assert_eq!(
        store
            .execute(
                Command::Query {
                    table_id: "events".into(),
                    query: next
                },
                None,
                0
            )
            .unwrap()["rows"][0]["key"],
        "event-1002"
    );
}
#[test]
fn context_values_share_storage_semantics_and_simulation_clones_are_isolated() {
    let mut live = MemoryStore::default();
    live.execute(command(json!({"operation":"create_table","definition":{"id":"context","name":"Context","kind":"context"}})),Some("context-create"),0).unwrap();
    live.execute(
        command(
            json!({"operation":"put","tableId":"context","key":"state","value":[1,"two",null]}),
        ),
        Some("context-write"),
        0,
    )
    .unwrap();
    let mut overlay = live.clone();
    overlay.execute(command(json!({"operation":"put","tableId":"context","key":"state","value":{"simulation":true}})),Some("simulation-write"),0).unwrap();
    assert_eq!(
        live.execute(
            command(json!({"operation":"get","tableId":"context","key":"state"})),
            None,
            0
        )
        .unwrap()["row"]["value"],
        json!([1, "two", null])
    );
    assert_eq!(
        overlay
            .execute(
                command(json!({"operation":"get","tableId":"context","key":"state"})),
                None,
                0
            )
            .unwrap()["row"]["value"],
        json!({"simulation":true})
    );
}
#[test]
fn malformed_schema_and_unbounded_queries_are_rejected() {
    assert!(
        serde_json::from_value::<Command>(
            json!({"operation":"get","tableId":"events","key":"a","unexpected":true})
        )
        .is_err()
    );
    let mut store = MemoryStore::default();
    create(&mut store);
    assert_eq!(
        store
            .execute(insert("event", json!({"active":true})), None, 0)
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        store
            .execute(
                Command::Query {
                    table_id: "events".into(),
                    query: Query {
                        limit: 101,
                        ..Default::default()
                    }
                },
                None,
                0
            )
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        store
            .execute(
                insert("event", json!({})),
                Some("missing-required-field"),
                0
            )
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );
}
#[test]
fn query_memory_and_result_budgets_preserve_continuation_for_large_rows() {
    let mut store = MemoryStore::default();
    create(&mut store);
    for i in 0..50 {
        store
            .execute(
                insert(
                    &format!("row-{i:03}"),
                    json!({"active":true,"content":"x".repeat(30_000)}),
                ),
                Some(&format!("large-{i}")),
                0,
            )
            .unwrap();
    }
    let mut cursor = None;
    let mut keys = Vec::new();
    loop {
        let result = store
            .execute(
                Command::Query {
                    table_id: "events".into(),
                    query: Query {
                        cursor,
                        limit: 100,
                        filters: vec![],
                    },
                },
                None,
                0,
            )
            .unwrap();
        assert!(result.to_string().len() < 65_536);
        assert!(!result["rows"].as_array().unwrap().is_empty());
        keys.extend(
            result["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["key"].as_str().unwrap().to_string()),
        );
        cursor = result["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(keys.len(), 50);
    assert_eq!(
        keys.iter().collect::<std::collections::BTreeSet<_>>().len(),
        50
    );
}
#[test]
fn integer_comparison_does_not_round_large_ids_through_floating_point() {
    let mut store = MemoryStore::default();
    create(&mut store);
    store
        .execute(
            insert(
                "large",
                json!({"active":true,"number":9_007_199_254_740_993u64}),
            ),
            Some("large-number"),
            0,
        )
        .unwrap();
    let result = store
        .execute(
            Command::Query {
                table_id: "events".into(),
                query: Query {
                    filters: vec![Filter {
                        pointer: "/number".into(),
                        operator: Operator::Gt,
                        value: json!(9_007_199_254_740_992u64),
                    }],
                    ..Default::default()
                },
            },
            None,
            0,
        )
        .unwrap();
    assert_eq!(result["rows"].as_array().unwrap().len(), 1);
}
#[test]
fn partial_updates_preserve_unrelated_fields_expiry_and_reject_stale_versions() {
    let mut store = MemoryStore::default();
    create(&mut store);
    let inserted=store.execute(command(json!({"operation":"insert_if_absent","tableId":"events","key":"partial","value":{"active":true,"token":"synthetic-secret","obsolete":1},"ttlMs":1000})),Some("patch-original"),10).unwrap();
    let version = inserted["row"]["version"].clone();
    let action = command(
        json!({"operation":"patch","tableId":"events","key":"partial","patch":{"active":false},"removeFields":["obsolete"],"expectedVersion":version}),
    );
    let updated = store
        .execute(action.clone(), Some("patch-update"), 20)
        .unwrap();
    assert_eq!(
        updated["row"]["value"],
        json!({"active":false,"token":"synthetic-secret"})
    );
    assert_eq!(updated["row"]["expiresAt"], 1010);
    assert_eq!(
        store.execute(action, Some("patch-update"), 30).unwrap(),
        updated
    );
    assert_eq!(store.execute(command(json!({"operation":"patch","tableId":"events","key":"partial","patch":{"active":true},"expectedVersion":version})),Some("patch-stale"),30).unwrap()["status"],"conflict");
    let invalid = command(
        json!({"operation":"patch","tableId":"events","key":"partial","patch":{},"removeFields":["active"]}),
    );
    assert_eq!(
        store
            .execute(invalid, Some("patch-required"), 30)
            .unwrap_err()
            .code,
        ErrorCode::InvalidRequest
    );
    let unchanged=store.execute(command(json!({"operation":"patch","tableId":"events","key":"partial","patch":{"active":false}})),Some("patch-noop"),30).unwrap();
    assert_eq!(unchanged["status"], "unchanged");
    assert_eq!(unchanged["row"]["version"], updated["row"]["version"]);
}

#[test]
fn receipt_completion_clock_survives_replay_and_legacy_deserialization() {
    let mut store = MemoryStore::default();
    create(&mut store);
    let action = insert("timed", json!({"active":true}));
    store
        .execute(action.clone(), Some("timed-write"), 10)
        .unwrap();
    store.execute(action, Some("timed-write"), 1000).unwrap();
    assert_eq!(store.receipts["timed-write"].completed_at, Some(10));

    let batch = vec![insert("batch-timed", json!({"active":true}))];
    store
        .execute_batch(batch.clone(), "timed-batch", 20)
        .unwrap();
    store.execute_batch(batch, "timed-batch", 2000).unwrap();
    assert_eq!(store.receipts["timed-batch"].completed_at, Some(20));

    // Missing timestamps are unknown, not epoch zero or recovery wall time.
    let legacy: workflow_data::Receipt = serde_json::from_value(json!({
        "key":"legacy", "fingerprint":"original", "result":{"status":"inserted"}
    }))
    .unwrap();
    assert_eq!(legacy.completed_at, None);
    let serialized = serde_json::to_value(&store.receipts["timed-write"]).unwrap();
    let restored: workflow_data::Receipt = serde_json::from_value(serialized).unwrap();
    assert_eq!(restored.completed_at, Some(10));
}
