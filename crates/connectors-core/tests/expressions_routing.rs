use connector_sdk::{Connector, FailureCode, Outcome, Task};
use connectors_core::CoreConnector;
use serde_json::{Value, json};
fn task(kind: &str, config: Value, input: Value) -> Task {
    serde_json::from_value(json!({"id":"task","idempotencyKey":"run:node","node":{"id":"node","type":kind,"label":"Test","config":config},"input":input,"attempt":1,"deadlineMs":1000,"context":{"nodes":{"prior":{"amount":5}},"trigger":{"original":true},"run":{"id":"run","startedAtMs":10}}})).unwrap()
}
fn output(expr: Value, input: Value) -> Value {
    match CoreConnector.execute(&task(
        "Expressions",
        json!({"fields":{"result":expr}}),
        input,
    )) {
        Outcome::Complete { output, .. } => output["result"].clone(),
        other => panic!("{other:?}"),
    }
}
fn lit(v: Value) -> Value {
    json!({"op":"literal","value":v})
}
fn failed(expr: Value, code: FailureCode) {
    assert!(
        matches!(CoreConnector.execute(&task("Expressions",json!({"fields":{"result":expr}}),json!({}))),Outcome::Failed{failure} if failure.code==code)
    );
}
#[test]
fn expressions_preserve_types_missing_and_exact_integer_arithmetic() {
    assert_eq!(
        output(
            json!({"op":"add","args":[lit(json!(9007199254740993u64)),lit(json!(1))]}),
            Value::Null
        ),
        json!(9007199254740994u64)
    );
    assert_eq!(
        output(
            json!({"op":"node","nodeId":"prior","path":"/amount"}),
            Value::Null
        ),
        5
    );
    assert_eq!(
        output(json!({"op":"trigger","path":"/original"}), Value::Null),
        true
    );
    assert_eq!(
        output(json!({"op":"run","path":"/startedAtMs"}), Value::Null),
        10
    );
    assert_eq!(
        output(
            json!({"op":"exists","args":[{"op":"input","path":"/missing"}]}),
            json!({})
        ),
        false
    );
    assert_eq!(
        output(
            json!({"op":"exists","args":[{"op":"input","path":"/null"}]}),
            json!({"null":null})
        ),
        true
    );
    assert_eq!(
        output(
            json!({"op":"coalesce","args":[{"op":"input","path":"/missing"},lit(Value::Null),lit(json!(false))]}),
            json!({})
        ),
        false
    );
    // Unselected branches and short-circuited arguments are not evaluated.
    assert_eq!(
        output(
            json!({"op":"if","args":[lit(json!(true)),lit(json!("ok")),{"op":"input","path":"/missing"}]}),
            json!({})
        ),
        "ok"
    );
    failed(
        json!({"op":"input","path":"/missing"}),
        FailureCode::MissingInput,
    );
    failed(
        json!({"op":"add","args":[lit(json!("2")),lit(json!(1))]}),
        FailureCode::InvalidConfig,
    );
    failed(
        json!({"op":"add","args":[lit(json!(u64::MAX)),lit(json!(1))]}),
        FailureCode::BudgetExceeded,
    );
    failed(
        json!({"op":"divide","args":[lit(json!(1)),lit(json!(0))]}),
        FailureCode::InvalidConfig,
    );
}
#[test]
fn strings_dates_and_budgets_are_portable_and_bounded() {
    assert_eq!(
        output(
            json!({"op":"to_text","args":[lit(json!(9007199254740993u64))]}),
            Value::Null
        ),
        "9007199254740993"
    );
    let date = json!({"op":"date_parse","args":[lit(json!("2026-01-01T01:00:00+01:00"))]});
    assert_eq!(
        output(
            json!({"op":"date_iso","args":[{"op":"date_add_ms","args":[date,lit(json!(60000))]}]}),
            Value::Null
        ),
        "2026-01-01T00:01:00.000Z"
    );
    assert_eq!(
        output(
            json!({"op":"concat","args":[{"op":"upper","args":[lit(json!("hello"))]},lit(json!(" world"))]}),
            Value::Null
        ),
        "HELLO world"
    );
    assert_eq!(
        output(
            json!({"op":"length","args":[lit(json!("😀a"))]}),
            Value::Null
        ),
        2
    );
    failed(
        json!({"op":"date_parse","args":[lit(json!("yesterday"))]}),
        FailureCode::InvalidConfig,
    );
    failed(
        json!({"op":"host_eval","args":[]}),
        FailureCode::InvalidConfig,
    );
    failed(
        json!({"op":"input","path":"/~bad"}),
        FailureCode::InvalidConfig,
    );
    let mut deep = lit(json!(true));
    for _ in 0..14 {
        deep = json!({"op":"not","args":[deep]});
    }
    failed(deep, FailureCode::BudgetExceeded);
    failed(
        json!({"op":"concat","args":[lit(json!("x".repeat(40000))),lit(json!("x".repeat(40000)))]}),
        FailureCode::BudgetExceeded,
    );
    let many: serde_json::Map<String, Value> = (0..32)
        .map(|i| {
            (
                i.to_string(),
                json!({"op":"concat","args":(0..16).map(|_|lit(json!("x"))).collect::<Vec<_>>()}),
            )
        })
        .collect();
    assert!(
        connectors_core::expressions::validate(json!({"fields":many}).as_object().unwrap())
            .is_err()
    );
}
#[test]
fn switch_uses_first_match_and_fallback_with_validated_names() {
    let rule = |port: &str, n: i64| json!({"port":port,"condition":{"match":"all","conditions":[{"field":"amount","operator":"greater_than","value":n}]}});
    let config = json!({"rules":[rule("large",100),rule("medium",10)]});
    for (amount, port) in [(101, "large"), (20, "medium"), (1, "fallback")] {
        assert_eq!(
            CoreConnector.execute(&task(
                "Switch / Router",
                config.clone(),
                json!({"amount":amount})
            )),
            Outcome::Complete {
                output: json!({"amount":amount}),
                port: port.into()
            }
        );
    }
    assert!(
        connectors_core::routing::validate_switch(
            json!({"rules":[rule("fallback",0)]}).as_object().unwrap()
        )
        .is_err()
    );
    assert!(
        connectors_core::routing::validate_switch(
            json!({"rules":[rule("same",0),rule("same",1)]})
                .as_object()
                .unwrap()
        )
        .is_err()
    );
}
#[test]
fn merge_order_keys_conflicts_and_skipped_branches_are_explicit() {
    let input =
        json!({"b":[{"id":"b","stock":2},{"id":"a","stock":3}],"a":[{"id":"a","name":"A"}]});
    let config = json!({"operation":"by_key","sources":["a","b"],"keyPointer":"/id"});
    assert_eq!(
        CoreConnector.execute(&task("Merge", config.clone(), input.clone())),
        Outcome::success(json!([{"id":"a","name":"A","stock":3},{"id":"b","stock":2}]))
    );
    assert_eq!(
        CoreConnector.execute(&task(
            "Merge",
            json!({"operation":"append","sources":["b","a"]}),
            input.clone()
        )),
        Outcome::success(json!([{"id":"b","stock":2},{"id":"a","stock":3},{"id":"a","name":"A"}]))
    );
    assert_eq!(
        CoreConnector.execute(&task(
            "Merge",
            json!({"operation":"select","sources":["a"]}),
            input
        )),
        Outcome::success(json!([{"id":"a","name":"A"}]))
    );
    let conflict = json!({"a":[{"id":1,"name":"A"}],"b":[{"id":1,"name":"B"}]});
    assert!(matches!(
        CoreConnector.execute(&task("Merge", config.clone(), conflict.clone())),
        Outcome::Failed { .. }
    ));
    let mut last = config;
    last["conflict"] = json!("last");
    assert_eq!(
        CoreConnector.execute(&task("Merge", last, conflict)),
        Outcome::success(json!([{"id":1,"name":"B"}]))
    );
    assert_eq!(
        CoreConnector.execute(&task(
            "Merge",
            json!({"operation":"append","sources":["missing"],"missing":"skip"}),
            json!({})
        )),
        Outcome::success(json!([]))
    );
    assert!(
        matches!(CoreConnector.execute(&task("Merge",json!({"operation":"append","sources":["missing"]}),json!({}))),Outcome::Failed{failure} if failure.code==FailureCode::MissingInput)
    );
}
