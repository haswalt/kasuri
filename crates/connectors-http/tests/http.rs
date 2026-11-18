use connector_sdk::{FailureCode, Outcome, Task};
use connectors_http::*;
use serde_json::{Value, json};
fn task(config: Value) -> Task {
    Task {
        id: "task".into(),
        idempotency_key: "stable".into(),
        node: serde_json::from_value(
            json!({"id":"http","type":"HTTP Request","label":"HTTP","config":config}),
        )
        .unwrap(),
        input: json!({"term":"a & b","payload":{"name":"Ada"}}),
        attempt: 1,
        deadline_ms: 1000,
    }
}
#[test]
fn constructs_queries_and_body_from_input_without_interpolation() {
    let task = task(
        json!({"url":"https://api.example.com/data","method":"POST","query":{"q":{"$input":"/term"}},"body":{"record":{"$input":"/payload"}}}),
    );
    let spec = validate(&task).unwrap();
    assert!(spec.url.contains("q=a+%26+b"));
    assert_eq!(spec.body.unwrap(), r#"{"record":{"name":"Ada"}}"#);
}
#[test]
fn denies_private_hosts_credentials_redirect_targets_and_unapproved_egress() {
    for url in [
        "http://example.com",
        "https://127.0.0.1",
        "https://[::1]",
        "https://2130706433",
        "https://localhost",
        "https://foo.internal",
        "https://user:secret@example.com",
        "https://example.com:8443",
        "https://example.com/#fragment",
    ] {
        assert!(validate(&task(json!({"url":url}))).is_err(), "{url}");
    }
    let spec = validate(&task(json!({"url":"https://api.example.com"}))).unwrap();
    assert!(allowed(&spec, "other.example.com, api.example.com"));
    assert!(!allowed(&spec, "example.com"));
    assert!(!allowed(&spec, "*"));
    assert!(!allowed(&spec, ""));
    assert!(
        validate(&task(
            json!({"url":"https://api.example.com","headers":{"Authorization":"secret"}})
        ))
        .is_err()
    );
}
#[test]
fn writes_cannot_enable_automatic_retries() {
    let mut t = task(json!({"url":"https://api.example.com","method":"POST"}));
    t.node.retry.max_attempts = 2;
    assert!(validate(&t).is_err());
    t.node.config.insert("method".into(), json!("GET"));
    assert!(validate(&t).is_ok());
}
#[test]
fn classifies_statuses_bounds_bodies_and_omits_error_payloads() {
    let t = task(json!({"url":"https://api.example.com"}));
    let spec = validate(&t).unwrap();
    assert!(
        matches!(response(&t,&spec,429,None,b"secret"),Outcome::Failed{failure} if failure.retryable&&failure.code==FailureCode::External)
    );
    assert!(
        matches!(response(&t,&spec,302,None,b""),Outcome::Failed{failure} if !failure.retryable)
    );
    assert!(
        matches!(response(&t,&spec,200,None,&vec![b'x';MAX_RESPONSE_BYTES+1]),Outcome::Failed{failure} if failure.code==FailureCode::BudgetExceeded)
    );
    assert!(
        matches!(response(&t,&spec,200,None,b"invalid"),Outcome::Failed{failure} if failure.code==FailureCode::InvalidResponse)
    );
    assert_eq!(
        response(&t, &spec, 204, None, b""),
        Outcome::success(json!({"status":204,"contentType":null,"body":null}))
    );
}
#[test]
fn simulations_require_an_explicit_fixture_and_use_the_same_response_parser() {
    let t = task(
        json!({"url":"https://api.example.com","simulationResponse":"{\"status\":200,\"body\":{\"ok\":true}}"}),
    );
    assert!(matches!(simulate(&t),Outcome::Complete{output,..} if output["body"]["ok"]==true));
    assert!(
        matches!(simulate(&task(json!({"url":"https://api.example.com"}))),Outcome::Failed{failure} if failure.code==FailureCode::MissingInput)
    );
}
