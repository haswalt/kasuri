use connector_sdk::Outcome;
use connectors_core::collections::execute;
use serde_json::json;
#[test]
fn batches_and_aggregate_preserve_order_and_integer_precision() {
    let config = json!({"arrayPointer":"/values","batchSize":2});
    assert_eq!(
        execute(
            "Split / Batch",
            config.as_object().unwrap(),
            &json!({"values":[1,2,3,4,5]})
        ),
        Outcome::success(json!({"batches":[[1,2],[3,4],[5]],"total":5,"batchSize":2}))
    );
    assert_eq!(
        execute(
            "Aggregate",
            json!({"operation":"sum"}).as_object().unwrap(),
            &json!([9007199254740993u64, 2])
        ),
        Outcome::success(json!(9007199254740995u64))
    );
    assert_eq!(
        execute(
            "Aggregate",
            json!({"operation":"collect","pointer":"/output","missing":"skip"})
                .as_object()
                .unwrap(),
            &json!([{"output":1},{"error":"external"},{"output":3}])
        ),
        Outcome::success(json!([1, 3]))
    );
}
#[test]
fn empty_collections_and_bad_values_are_explicit() {
    assert_eq!(
        execute("Split / Batch", json!({}).as_object().unwrap(), &json!([])),
        Outcome::success(json!({"batches":[],"total":0,"batchSize":10}))
    );
    assert_eq!(
        execute(
            "Aggregate",
            json!({"operation":"sum"}).as_object().unwrap(),
            &json!([])
        ),
        Outcome::success(json!(0))
    );
    for (kind, config, input) in [
        ("Split / Batch", json!({"batchSize":0}), json!([])),
        ("Aggregate", json!({"operation":"sum"}), json!(["1"])),
        (
            "Aggregate",
            json!({"operation":"collect","pointer":"/missing"}),
            json!([{}]),
        ),
    ] {
        assert!(matches!(
            execute(kind, config.as_object().unwrap(), &input),
            Outcome::Failed { .. }
        ));
    }
}
