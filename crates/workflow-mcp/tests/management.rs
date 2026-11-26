use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use workflow_mcp::{auth::Auth, role, rpc};
use workflow_service::Workspace;
fn entropy() -> impl FnMut() -> String {
    let mut n = 0;
    move || {
        n += 1;
        format!("{n:064x}")
    }
}
fn call(ws: &mut Workspace, auth: &mut Auth, token: &str, name: &str, args: Value) -> Value {
    let c = auth
        .authenticate(token, "https://kasuri.example/mcp/test/mcp", 100)
        .unwrap();
    rpc(ws,auth,&c,json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}),100,"generated-id").unwrap()["result"].clone()
}
#[test]
fn scoped_tools_revision_conflicts_idempotency_and_audit_survive_restart() {
    let mut auth = Auth::default();
    let mut random = entropy();
    let resource = "https://kasuri.example/mcp/test/mcp";
    let builder = auth
        .issue(
            "Builder",
            role("builder"),
            resource,
            0,
            86400000,
            &mut random,
        )
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let reader = auth
        .issue("Reader", role("reader"), resource, 0, 86400000, &mut random)
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = Workspace::default();
    let graph: Value =
        serde_json::from_str(include_str!("../../../examples/workflows/01-hello.json")).unwrap();
    let create = json!({"workflow":graph,"idempotencyKey":"create-example"});
    let result = call(
        &mut ws,
        &mut auth,
        &builder,
        "workflows_create",
        create.clone(),
    );
    assert_eq!(result["isError"], false);
    ws.management = serde_json::to_value(&auth).unwrap();
    let mut ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    let mut auth: Auth = serde_json::from_value(ws.management.clone()).unwrap();
    assert_eq!(
        call(&mut ws, &mut auth, &builder, "workflows_create", create),
        result
    );
    assert_eq!(ws.workflows.len(), 1);
    assert_eq!(
        call(
            &mut ws,
            &mut auth,
            &reader,
            "workflows_create",
            json!({"workflow":graph,"idempotencyKey":"reader-denied"})
        )["isError"],
        true
    );
    let run = json!({"workflowId":"01-hello","expectedVersion":1,"input":{},"idempotencyKey":"simulate-example"});
    let first = call(&mut ws, &mut auth, &builder, "runs_simulate", run.clone());
    assert_eq!(first["isError"], false);
    assert_eq!(
        call(&mut ws, &mut auth, &builder, "runs_simulate", run),
        first
    );
    assert_eq!(ws.runs.len(), 1);
    assert_eq!(
        call(
            &mut ws,
            &mut auth,
            &builder,
            "runs_start",
            json!({"workflowId":"01-hello","expectedVersion":1,"input":{},"idempotencyKey":"execute-denied"})
        )["isError"],
        true
    );
    assert_eq!(
        call(
            &mut ws,
            &mut auth,
            &builder,
            "workflows_update",
            json!({"workflow":graph,"expectedVersion":0,"idempotencyKey":"stale-version"})
        )["structuredContent"]["code"],
        "conflict"
    );
    let mut updated = serde_json::to_value(&ws.workflows[0]).unwrap();
    updated["name"] = json!("Browser edit");
    ws.handle(
        "PUT",
        "/api/workflows/01-hello",
        &serde_json::to_vec(&updated).unwrap(),
        100,
        "browser-write",
    )
    .unwrap();
    assert_eq!(
        call(
            &mut ws,
            &mut auth,
            &builder,
            "workflows_update",
            json!({"workflow":graph,"expectedVersion":1,"idempotencyKey":"agent-stale"})
        )["structuredContent"]["code"],
        "conflict"
    );
    assert_eq!(
        ws.handle(
            "PUT",
            "/api/workflows/01-hello",
            &serde_json::to_vec(&updated).unwrap(),
            100,
            "browser-stale"
        )
        .unwrap_err()
        .status,
        409
    );
    assert!(ws.audit.iter().any(|e| e["outcome"] == "forbidden"));
    assert!(ws.audit.iter().any(|e| e["actor"] == "browser"));
    assert!(!ws.snapshot().unwrap().to_string().contains("tokenHash"));
    assert!(!serde_json::to_string(&auth).unwrap().contains(&builder));
    assert!(
        auth.authenticate(&builder, "https://other.example/mcp/test/mcp", 100)
            .is_err()
    );
}
#[test]
fn oauth_requires_explicit_consent_pkce_audience_and_rotating_refresh() {
    let mut auth = Auth::default();
    let mut random = entropy();
    let resource = "https://kasuri.example/mcp/test/mcp";
    assert!(
        auth.register(
            &json!({"redirect_uris":["https://app.example/callback#bad"]}),
            0,
            &mut random
        )
        .is_err()
    );
    assert!(
        auth.register(
            &json!({"redirect_uris":["http://public.example/callback"]}),
            0,
            &mut random
        )
        .is_err()
    );
    let client=auth.register(&json!({"client_name":"Test client","redirect_uris":["http://127.0.0.1:3000/callback"],"token_endpoint_auth_method":"none"}),0,&mut random).unwrap();
    let verifier = "v".repeat(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let request = json!({"client_id":client["client_id"],"redirect_uri":"http://127.0.0.1:3000/callback","resource":resource,"response_type":"code","code_challenge_method":"S256","code_challenge":challenge,"state":"client-csrf-state","scope":"read write simulate"});
    let id = auth.authorize(&request, resource, 0, &mut random).unwrap();
    assert!(auth.consent(&id, role("operator"), 0, &mut random).is_err());
    let redirect = auth
        .consent(&id, vec!["read".into()], 0, &mut random)
        .unwrap();
    let url = url::Url::parse(&redirect).unwrap();
    let code = url
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .to_string();
    let mut exchange = json!({"grant_type":"authorization_code","client_id":client["client_id"],"redirect_uri":"http://127.0.0.1:3000/callback","resource":resource,"code":code,"code_verifier":"wrong"});
    assert!(auth.exchange(&exchange, resource, 0, &mut random).is_err());
    exchange["code_verifier"] = json!(verifier);
    let result = auth.exchange(&exchange, resource, 0, &mut random).unwrap();
    assert_eq!(result["scope"], "read");
    assert!(auth.exchange(&exchange, resource, 0, &mut random).is_err());
    assert!(
        auth.authenticate(result["access_token"].as_str().unwrap(), resource, 1)
            .is_ok()
    );
    let refresh = json!({"grant_type":"refresh_token","client_id":client["client_id"],"refresh_token":result["refresh_token"],"resource":resource});
    let rotated = auth.exchange(&refresh, resource, 1, &mut random).unwrap();
    assert_ne!(rotated["refresh_token"], result["refresh_token"]);
    assert!(auth.exchange(&refresh, resource, 2, &mut random).is_err());
    assert!(
        auth.authenticate(rotated["access_token"].as_str().unwrap(), resource, 3)
            .is_err()
    );
}
#[test]
fn audit_retention_is_reported_and_credentials_are_revocable() {
    let mut ws = Workspace::default();
    for i in 0..1005 {
        ws.record_audit("agent", "workflow.updated", "workflow", "succeeded", i, "")
    }
    let page = ws.audit_page(0, 10);
    assert_eq!(page["oldestRetainedId"], 6);
    assert_eq!(page["totalRecorded"], 1005);
    assert_eq!(page["items"].as_array().unwrap().len(), 10);
    let mut auth = Auth::default();
    let issued = auth
        .issue(
            "reader",
            role("reader"),
            "resource",
            0,
            3600000,
            &mut entropy(),
        )
        .unwrap();
    let token = issued["token"].as_str().unwrap();
    assert!(auth.authenticate(token, "resource", 1).is_ok());
    auth.credentials[0].revoked = true;
    assert!(auth.authenticate(token, "resource", 2).is_err());
}
#[test]
fn malformed_tool_inputs_do_not_panic_or_execute() {
    let mut auth = Auth::default();
    let token = auth
        .issue(
            "test",
            role("operator"),
            "https://kasuri.example/mcp/test/mcp",
            0,
            3600000,
            &mut entropy(),
        )
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = Workspace::default();
    assert_eq!(
        call(
            &mut ws,
            &mut auth,
            &token,
            "workflows_validate",
            json!({"workflow":"not an object"})
        )["isError"],
        true
    );
    assert_eq!(
        call(
            &mut ws,
            &mut auth,
            &token,
            "workflows_create",
            json!({"workflow":[],"idempotencyKey":"bad-input"})
        )["isError"],
        true
    );
    assert!(ws.workflows.is_empty());
    let redacted = workflow_mcp::tools::redact(
        json!({"headers":"{\"Authorization\":\"Bearer secret\"}","password":"private"}),
    );
    assert!(!redacted.to_string().contains("Bearer secret"));
    assert!(!redacted.to_string().contains("private"));
}

#[test]
fn unused_oauth_clients_cannot_lock_out_registration_or_authorization() {
    let mut auth = Auth::default();
    let mut random = entropy();
    let resource = "https://kasuri.example/mcp/test/mcp";
    let register = |auth: &mut Auth, random: &mut dyn FnMut() -> String, now| {
        auth.register(
            &json!({"redirect_uris":["https://app.example/callback"]}),
            now,
            random,
        )
    };
    // A legitimate client completes authorization before the registry is flooded.
    let legit = register(&mut auth, &mut random, 0).unwrap();
    let verifier = "v".repeat(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let request = |client: &Value, state: &str| json!({"client_id":client["client_id"],"redirect_uri":"https://app.example/callback","resource":resource,"response_type":"code","code_challenge_method":"S256","code_challenge":challenge,"state":state,"scope":"read"});
    let id = auth
        .authorize(&request(&legit, "s"), resource, 0, &mut random)
        .unwrap();
    let redirect = auth
        .consent(&id, vec!["read".into()], 0, &mut random)
        .unwrap();
    let code = url::Url::parse(&redirect)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .to_string();
    auth.exchange(
        &json!({"grant_type":"authorization_code","code":code,"code_verifier":verifier,"client_id":legit["client_id"],"redirect_uri":"https://app.example/callback","resource":resource}),
        resource,
        0,
        &mut random,
    )
    .unwrap();
    // Flood the registry with clients that never authorize.
    for _ in 0..200 {
        register(&mut auth, &mut random, 1).unwrap();
    }
    assert!(auth.clients.len() <= 128);
    assert!(
        auth.clients.iter().any(|c| c.id == legit["client_id"]),
        "an authorized client is never evicted"
    );
    // Unused registrations expire within the hour; established ones are kept.
    register(&mut auth, &mut random, 2).unwrap();
    auth.prune(3_600_002);
    assert_eq!(auth.clients.len(), 1);
    assert_eq!(auth.clients[0].id, legit["client_id"]);
    // Pending authorization requests cannot be exhausted either.
    let flooder = register(&mut auth, &mut random, 3_600_003).unwrap();
    for i in 0..100 {
        auth.authorize(
            &request(&flooder, &format!("flood-{i}")),
            resource,
            3_600_003,
            &mut random,
        )
        .unwrap();
    }
    assert!(auth.requests.len() <= 64);
    assert!(
        auth.authorize(&request(&legit, "after"), resource, 3_600_004, &mut random)
            .is_ok()
    );
}

#[test]
fn clients_stored_before_establishment_tracking_keep_their_retention() {
    let auth: Auth = serde_json::from_value(json!({"clients":[{"id":"legacy","name":"Old","redirects":["https://app.example/cb"],"created":0}]})).unwrap();
    assert!(auth.clients[0].established);
    let mut auth = auth;
    auth.prune(30 * 86_400_000);
    assert_eq!(auth.clients.len(), 1);
}

#[test]
fn credential_hashes_keep_their_stored_encoding() {
    // Stored token hashes must keep the same lowercase hex across upgrades.
    assert_eq!(
        workflow_mcp::auth::hash("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn workflow_updates_keep_redacted_credentials_only_at_the_original_node_and_origin() {
    let mut auth = Auth::default();
    let builder = auth
        .issue(
            "Builder",
            role("builder"),
            "https://kasuri.example/mcp/test/mcp",
            0,
            86400000,
            &mut entropy(),
        )
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    for (id, headers) in [
        (
            "redacted-object",
            json!({"X-Api-Key":"synthetic-secret","X-Trace":"initial"}),
        ),
        (
            "redacted-json",
            json!(r#"{ "X-Api-Key": "synthetic-secret", "X-Trace": "initial" }"#),
        ),
    ] {
        let mut ws = Workspace::default();
        let mut workflow: Value = serde_json::from_str(include_str!(
            "../../../examples/workflows/06-public-http.json"
        ))
        .unwrap();
        workflow["id"] = json!(id);
        workflow["nodes"][1]["config"]["headers"] = headers.clone();
        let created = call(
            &mut ws,
            &mut auth,
            &builder,
            "workflows_create",
            json!({"workflow":workflow,"idempotencyKey":format!("{id}-create")}),
        );
        assert_eq!(created["isError"], false, "{created}");
        let read = call(
            &mut ws,
            &mut auth,
            &builder,
            "workflows_get",
            json!({"workflowId":id}),
        );
        assert!(!read.to_string().contains("synthetic-secret"));
        let mut edited = read["structuredContent"].clone();
        let version = edited["version"].clone();
        edited["name"] = json!("Enrich data");
        edited["nodes"].as_array_mut().unwrap().swap(1, 2);
        edited["nodes"][2]["config"]["url"] = json!("https://jsonplaceholder.typicode.com/posts/2");
        let update = |ws: &mut Workspace,
                      auth: &mut Auth,
                      workflow: &Value,
                      suffix: &str,
                      version: &Value| {
            call(
                ws,
                auth,
                &builder,
                "workflows_update",
                json!({"workflow":workflow,"expectedVersion":version,"idempotencyKey":format!("{id}-{suffix}")}),
            )
        };
        let mut moved = edited.clone();
        moved["nodes"][2]["config"]["url"] = json!("https://other.example/data");
        let denied = update(&mut ws, &mut auth, &moved, "new-origin", &version);
        assert_eq!(
            denied["structuredContent"]["code"],
            "redacted_configuration"
        );
        assert!(!denied.to_string().contains("synthetic-secret"));
        let mut misplaced = edited.clone();
        misplaced["nodes"][2]["config"]["headers"] = json!({"X-New-Api-Key":"[REDACTED]"});
        assert_eq!(
            update(&mut ws, &mut auth, &misplaced, "new-field", &version)["structuredContent"]["code"],
            "redacted_configuration"
        );
        let mut new_node = edited.clone();
        new_node["nodes"][2]["id"] = json!("new-fetch");
        assert_eq!(
            update(&mut ws, &mut auth, &new_node, "new-node", &version)["structuredContent"]["code"],
            "redacted_configuration"
        );
        assert_eq!(
            update(&mut ws, &mut auth, &edited, "stale", &json!(0))["structuredContent"]["code"],
            "conflict"
        );
        let saved = update(&mut ws, &mut auth, &edited, "same-field", &version);
        assert_eq!(saved["isError"], false, "{saved}");
        assert!(!saved.to_string().contains("synthetic-secret"));
        assert_eq!(
            update(&mut ws, &mut auth, &edited, "same-field", &version),
            saved
        );
        let stored = &ws.workflows[0];
        assert_eq!(stored.graph.name, "Enrich data");
        assert_eq!(
            stored
                .graph
                .nodes
                .iter()
                .find(|n| n.id == "fetch")
                .unwrap()
                .config["headers"],
            headers
        );
        let read = call(
            &mut ws,
            &mut auth,
            &builder,
            "workflows_get",
            json!({"workflowId":id}),
        );
        assert!(!read.to_string().contains("synthetic-secret"));
        let mut new_workflow = read["structuredContent"].clone();
        new_workflow["id"] = json!("no-saved-source");
        assert_eq!(
            call(
                &mut ws,
                &mut auth,
                &builder,
                "workflows_create",
                json!({"workflow":new_workflow,"idempotencyKey":format!("{id}-create-placeholder")})
            )["structuredContent"]["code"],
            "redacted_configuration"
        );
    }
}
