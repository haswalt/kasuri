use connector_sdk::{FailureCode, Outcome, Task};
use connectors_http::*;
use serde_json::{Value, json};
fn task(config: Value) -> Task {
    Task {
        context: Value::Null,
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
fn public_https_is_default_and_private_destinations_need_explicit_opt_in() {
    for url in [
        "http://example.com",
        "https://user:secret@example.com",
        "https://example.com/#fragment",
    ] {
        assert!(validate(&task(json!({"url":url}))).is_err());
    }
    for url in [
        "https://127.0.0.1",
        "https://[::1]",
        "https://2130706433",
        "https://169.254.169.254/latest/meta-data",
        "https://foo.internal",
        "https://localhost",
        "https://[::ffff:10.1.2.3]",
    ] {
        let spec = validate(&task(json!({"url":url}))).unwrap();
        assert!(!public_destination(&spec), "{url}");
        assert!(!private_opt_in(&spec, ""));
    }
    let spec = validate(&task(json!({"url":"https://api.example.com:8443"}))).unwrap();
    assert!(allowed(&spec, ""));
    assert!(allowed(&spec, "other.example.com, api.example.com"));
    assert!(!allowed(&spec, "example.com"));
    assert!(!allowed(&spec, "*"));
    assert!(public_destination(&spec));
    let private = validate(&task(json!({"url":"https://api.internal"}))).unwrap();
    assert!(private_opt_in(&private, "api.internal"));
}
#[test]
fn custom_headers_basic_auth_and_secret_references_are_resolved_only_at_dispatch() {
    let t = task(
        json!({"url":"https://api.example.com","authentication":"basic","username":"user","passwordSecret":"password","headers":{"X-Trace":"request-123","X-Api-Key":{"$secret":"api_key"},"X-Tenant":"team-a"}}),
    );
    let spec = validate(&t).unwrap();
    assert!(!spec.headers.contains_key("authorization"));
    let secrets = std::collections::BTreeMap::from([
        ("password".into(), "pass:word".into()),
        ("api_key".into(), "private-key".into()),
    ]);
    let (headers, redactions) = authorization_headers(&spec, &secrets).unwrap();
    assert_eq!(headers["authorization"], "Basic dXNlcjpwYXNzOndvcmQ=");
    assert_eq!(headers["x-api-key"], "private-key");
    assert_eq!(headers["x-trace"], "request-123");
    let mut output =
        json!({"echo":"Basic dXNlcjpwYXNzOndvcmQ=","nested":["private-key","user:pass:word"]});
    redact(&mut output, &redactions);
    assert!(!output.to_string().contains("private-key"));
    assert!(!output.to_string().contains("pass:word"));
    assert!(!output.to_string().contains("dXNlcj"));
    assert!(authorization_headers(&spec, &std::collections::BTreeMap::new()).is_err());
}
#[test]
fn refuses_header_injection_transport_headers_and_literal_credentials() {
    for (name, value) in [
        ("Host", json!("attacker.example")),
        ("Content-Length", json!("0")),
        ("Connection", json!("upgrade")),
        ("X-Forwarded-For", json!("127.0.0.1")),
        ("Bad Header", json!("x")),
        ("X-Trace", json!("one\r\ninjected: yes")),
        ("Authorization", json!("Bearer plaintext")),
        ("X-Api-Key", json!("plaintext")),
    ] {
        assert!(
            validate(&task(
                json!({"url":"https://api.example.com","headers":{name:value}})
            ))
            .is_err(),
            "{name}"
        );
    }
    assert!(validate(&task(json!({"url":"https://api.example.com","headers":{"Authorization":{"$secret":"bearer"}}}))).is_ok());
    assert!(validate(&task(json!({"url":"https://api.example.com","authentication":"basic","username":"u","passwordSecret":"p","headers":{"Authorization":{"$secret":"bearer"}}}))).is_err());
}
#[test]
fn private_and_reserved_ip_ranges_are_not_public() {
    for ip in [
        "0.0.0.0",
        "10.0.0.1",
        "100.64.0.1",
        "127.0.0.1",
        "169.254.169.254",
        "172.16.0.1",
        "192.168.0.1",
        "198.18.0.1",
        "224.0.0.1",
        "::1",
        "fc00::1",
        "fe80::1",
        "::ffff:127.0.0.1",
        "2001:db8::1",
        "64:ff9b::a00:1",
    ] {
        assert!(!public_ip(ip.parse().unwrap()), "{ip}");
    }
    for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
        assert!(public_ip(ip.parse().unwrap()), "{ip}");
    }
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

#[test]
fn redaction_does_not_reprocess_replacement_markers_or_expand_recursively() {
    let mut output = json!({"text":"a[b","a":"b"});
    redact(&mut output, &["a".into(), "[".into(), "b".into()]);
    assert_eq!(
        output,
        json!({"text":"[REDACTED][REDACTED][REDACTED]","[REDACTED]":"[REDACTED]"})
    );
}
