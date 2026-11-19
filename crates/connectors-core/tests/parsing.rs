use connector_sdk::Outcome;
use connectors_core::parsing::{execute, validate};
use serde_json::{Value, json};
fn parse(format: &str, input: Value) -> Value {
    match execute(json!({"format":format}).as_object().unwrap(), &input) {
        Outcome::Complete { output, .. } => output,
        other => panic!("{other:?}"),
    }
}
#[test]
fn json_text_forms_and_xml_preserve_explicit_shapes() {
    assert_eq!(
        parse("json", json!("{\"id\":9007199254740993}")),
        json!({"valid":true,"value":{"id":9007199254740993u64},"errors":[]})
    );
    assert_eq!(parse("json", json!({"id":1}))["value"], json!({"id":1}));
    assert_eq!(parse("text", json!("  hello\n"))["value"], "  hello\n");
    assert_eq!(
        parse(
            "form",
            json!("name=Ada+Lovelace&tag=one&tag=two&empty=&unicode=%E2%9C%93")
        )["value"],
        json!({"name":["Ada Lovelace"],"tag":["one","two"],"empty":[""],"unicode":["✓"]})
    );
    let xml = parse(
        "xml",
        json!("<a xmlns='urn:example' id='1'>hello<b/>world &amp; more</a>"),
    );
    assert_eq!(xml["valid"], true);
    assert_eq!(xml["value"]["name"], "a");
    assert_eq!(xml["value"]["namespace"], "urn:example");
    assert_eq!(xml["value"]["children"][1]["name"], "b");
    assert_eq!(xml["value"]["children"][2], "world & more");
}
#[test]
fn bad_data_is_routable_and_diagnostics_never_echo_input() {
    for (format, text) in [
        ("json", "private-bad-json"),
        ("xml", "<private>"),
        (
            "xml",
            "<!DOCTYPE x [<!ENTITY leak SYSTEM 'file:///private'>]><x>&leak;</x>",
        ),
        ("form", "x=%ff"),
        ("form", "x=%GG"),
    ] {
        let output = parse(format, json!(text));
        assert_eq!(output["valid"], false);
        assert_eq!(output["value"], Value::Null);
        assert!(!output.to_string().contains("private"));
    }
    assert_eq!(
        parse(
            "xml",
            json!(format!("{}{}", "<a>".repeat(34), "</a>".repeat(34)))
        )["errors"][0]["code"],
        "too_deep"
    );
    assert_eq!(parse("text", json!("a".repeat(32769)))["valid"], false);
    assert!(validate(json!({"format":"yaml"}).as_object().unwrap()).is_err());
    assert!(
        validate(
            json!({"format":"xml","resolveEntities":true})
                .as_object()
                .unwrap()
        )
        .is_err()
    );
}
#[test]
fn selected_body_and_missing_paths_are_explicit() {
    let config = json!({"format":"json","pointer":"/body"});
    let output = execute(config.as_object().unwrap(), &json!({"body":"[1,2]"}));
    assert_eq!(
        output,
        Outcome::success(json!({"valid":true,"value":[1,2],"errors":[]}))
    );
    assert_eq!(
        execute(config.as_object().unwrap(), &json!({})),
        Outcome::success(json!({"valid":false,"value":null,"errors":[{"code":"missing_input"}]}))
    );
}
