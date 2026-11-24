use serde_json::{Value, json};
use workflow_service::Workspace;

fn hook(config: Value) -> Value {
    json!({"schemaVersion":1,"id":"flow","name":"Hook","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Webhook","label":"Start","config":config},{"id":"wait","type":"Wait / Delay","label":"Wait","config":{"delayMs":10}}],"edges":[{"id":"e","source":"start","target":"wait"}]})
}
fn save(ws: &mut Workspace, doc: &Value) {
    ws.handle(
        "PUT",
        "/api/workflows/flow",
        &serde_json::to_vec(doc).unwrap(),
        0,
        "",
    )
    .unwrap();
}
fn publish(ws: &mut Workspace) {
    ws.handle("POST", "/api/workflows/flow/publish", b"", 0, "")
        .unwrap();
}

#[test]
fn auth_descriptors_follow_publication_and_live_trigger_configuration() {
    let mut ws = Workspace::default();
    save(
        &mut ws,
        &hook(json!({"authentication":"query","queryParameter":"secret_key"})),
    );
    ws.configure_webhook("flow", "endpoint-1".into(), "hash-1".into())
        .unwrap();
    let auth = ws.webhook_auth();
    assert_eq!(auth.len(), 1);
    assert_eq!(auth[0].state, "missing", "not yet published");
    publish(&mut ws);
    let auth = &ws.webhook_auth()[0];
    assert_eq!(auth.state, "active");
    assert_eq!(auth.authentication, "query");
    assert_eq!(auth.query_parameter, "secret_key");
    assert_eq!(auth.token_hash, "hash-1");
    assert_eq!(auth.endpoint_id, "endpoint-1");
    // A draft change does not alter live authentication until it is published.
    save(
        &mut ws,
        &hook(json!({"authentication":"hmac_sha256","signingSecret":"incoming"})),
    );
    assert_eq!(ws.webhook_auth()[0].authentication, "query");
    publish(&mut ws);
    let auth = &ws.webhook_auth()[0];
    assert_eq!(auth.authentication, "hmac_sha256");
    assert_eq!(auth.signing_secret, "incoming");
    assert_eq!(auth.query_parameter, "key", "default parameter");
    // Rotation replaces the registration's hash.
    ws.configure_webhook("flow", "endpoint-2".into(), "hash-2".into())
        .unwrap();
    let auth = ws.webhook_auth();
    assert_eq!(auth.len(), 1);
    assert_eq!(
        (auth[0].endpoint_id.as_str(), auth[0].token_hash.as_str()),
        ("endpoint-2", "hash-2")
    );
}

#[test]
fn omitted_live_authentication_defaults_to_bearer_like_delivery() {
    let mut ws = Workspace::default();
    save(&mut ws, &hook(json!({})));
    ws.configure_webhook("flow", "endpoint".into(), "hash".into())
        .unwrap();
    publish(&mut ws);
    // Model a stored pre-upgrade trigger whose live graph omits authentication.
    let mut stored: Value = serde_json::from_str(&ws.checkpoint().unwrap()).unwrap();
    for revision in stored["revisions"].as_array_mut().unwrap() {
        if let Some(nodes) = revision
            .pointer_mut("/graph/nodes")
            .and_then(Value::as_array_mut)
        {
            for node in nodes {
                node["config"]
                    .as_object_mut()
                    .unwrap()
                    .remove("authentication");
            }
        }
    }
    let ws = Workspace::from_json(&stored.to_string()).unwrap();
    assert_eq!(ws.webhook_auth()[0].authentication, "bearer");
    assert_eq!(ws.webhook_info("flow").unwrap()["authentication"], "bearer");
}
