use connector_sdk::FailureCode;
use connectors_core::conditions::{evaluate, validate};
use serde_json::{Value, json};
fn evaluate_config(config: Value, input: Value) -> Result<bool, FailureCode> {
    evaluate(config.as_object().unwrap(), &input)
}
#[test]
fn typed_business_rules_match_all_or_any_and_keep_integer_precision() {
    let input = json!({"headers":{"content-type":"application/json; charset=utf-8"},"amount":120,"inserted":true,"id":9007199254740993u64});
    let rows = json!([{"field":"headers.content-type","operator":"starts_with","value":"application/json"},{"field":"/amount","operator":"greater_or_equal","valueType":"number","value":"100"},{"field":"inserted","operator":"equals","valueType":"boolean","value":true}]);
    assert_eq!(
        evaluate_config(json!({"match":"all","conditions":rows}), input.clone()),
        Ok(true)
    );
    assert_eq!(
        evaluate_config(
            json!({"match":"any","conditions":[{"field":"absent","operator":"exists"},{"field":"id","operator":"greater_than","value":9007199254740992u64}]}),
            input
        ),
        Ok(true)
    );
    assert_eq!(
        evaluate_config(
            json!({"conditions":[{"field":"$","operator":"equals","value":1.0}]}),
            json!(1)
        ),
        Ok(true)
    );
    assert_eq!(
        evaluate_config(
            json!({"conditions":[{"field":"$","operator":"equals","value":true}]}),
            json!("true")
        ),
        Ok(false)
    );
}
#[test]
fn existence_empty_membership_and_missing_comparisons_are_explicit() {
    let input = json!({"present":null,"empty":[],"tags":["paid",{"id":1}],"a/b":{"~key":"value"}});
    for row in [
        json!({"field":"present","operator":"exists"}),
        json!({"field":"missing","operator":"not_exists"}),
        json!({"field":"empty","operator":"is_empty"}),
        json!({"field":"tags","operator":"contains","valueType":"json","value":"{\"id\":1}"}),
        json!({"field":"/a~1b/~0key","operator":"ends_with","value":"ue"}),
    ] {
        assert_eq!(
            evaluate_config(json!({"conditions":[row]}), input.clone()),
            Ok(true)
        );
    }
    assert_eq!(
        evaluate_config(
            json!({"conditions":[{"field":"missing","operator":"not_equals","value":1}]}),
            input
        ),
        Ok(false)
    );
}
#[test]
fn invalid_rows_fail_validation_and_legacy_behavior_is_preserved() {
    for config in [
        json!({"conditions":[]}),
        json!({"conditions":[{"field":"a","operator":"greater_than","valueType":"number","value":"oops"}]}),
        json!({"conditions":[{"field":"/a~x","operator":"exists"}]}),
        json!({"conditions":[{"field":"a","operator":"eval","value":"code"}]}),
        json!({"conditions":[{"field":"a","operator":"equals","value":1}],"pointer":"/a","equals":1}),
    ] {
        assert!(validate(config.as_object().unwrap()).is_err(), "{config}");
    }
    assert_eq!(
        evaluate_config(
            json!({"pointer":"/flag","equals":true}),
            json!({"flag":true})
        ),
        Ok(true)
    );
    assert_eq!(
        evaluate_config(json!({"pointer":"/flag","equals":true}), json!({})),
        Err(FailureCode::MissingInput)
    );
}

#[test]
fn collection_rules_are_typed_and_preserve_legacy_filtering() {
    use connectors_core::conditions::{filter, validate_filter};
    let items = vec![
        json!({"active":true,"rank":3}),
        json!({"active":false,"rank":2}),
        json!({"active":true,"rank":1}),
    ];
    let before = items.clone();
    let config = json!({"arrayPointer":"/items","match":"all","conditions":[{"field":"active","operator":"equals","valueType":"boolean","value":true},{"field":"rank","operator":"greater_than","valueType":"number","value":1}]});
    assert_eq!(
        filter(config.as_object().unwrap(), &items).unwrap(),
        vec![items[0].clone()]
    );
    assert_eq!(items, before);
    let legacy = json!({"pointer":"/active","equals":"true"});
    assert_eq!(
        filter(legacy.as_object().unwrap(), &items).unwrap(),
        vec![items[0].clone(), items[2].clone()]
    );
    assert!(
        validate_filter(
            json!({"conditions":[],"pointer":"/active","equals":true})
                .as_object()
                .unwrap()
        )
        .is_err()
    );
}
