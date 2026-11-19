use serde_json::{Value, json};
use workflow_service::Workspace;
fn call(ws: &mut Workspace, method: &str, path: &str, body: Value, now: u64) -> Value {
    ws.handle(
        method,
        path,
        &serde_json::to_vec(&body).unwrap(),
        now,
        "test-run",
    )
    .unwrap()
    .1
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../examples/workflows/01-hello.json")).unwrap()
}
fn save(ws: &mut Workspace, value: Value, now: u64) -> Value {
    call(ws, "PUT", "/api/workflows/01-hello", value, now)
}
fn action(ws: &mut Workspace, name: &str, doc: &Value, now: u64) -> Value {
    call(
        ws,
        "POST",
        &format!("/api/workflows/01-hello/{name}"),
        json!({"expectedVersion":doc["version"]}),
        now,
    )
}
#[test]
fn unchanged_publication_preserves_revision_version_and_paused_state() {
    let mut ws = Workspace::default();
    let doc = save(&mut ws, fixture(), 0);
    let doc = action(&mut ws, "publish", &doc, 1);
    let mut doc = action(&mut ws, "pause", &doc, 2);
    doc["nodes"].as_array_mut().unwrap().reverse();
    doc["nodes"][0]["position"] = json!({"x":99,"y":123});
    doc["edges"].as_array_mut().unwrap().reverse();
    doc["edges"][0]["id"] = json!("new-editor-identity");
    let saved = save(&mut ws, doc, 3);
    assert_eq!(saved["status"], "paused");
    assert_eq!(saved["hasUnpublishedChanges"], false);
    let published = action(&mut ws, "publish", &saved, 4);
    assert_eq!(published, saved);
    assert_eq!(ws.revisions.len(), 1);
    assert_eq!(ws.revisions[0].published_at, "1970-01-01T00:00:00.001Z");
}
#[test]
fn active_cron_uses_immutable_definition_after_invalid_draft_save() {
    let mut ws = Workspace::default();
    let mut input = fixture();
    input["nodes"][0]["type"] = json!("Schedule");
    input["nodes"][0]["config"] = json!({"cron":"* * * * *","timezone":"UTC"});
    let doc = save(&mut ws, input, 0);
    let mut doc = action(&mut ws, "publish", &doc, 0);
    doc["nodes"][0]["config"]["cron"] = json!("invalid draft");
    doc["name"] = json!("Unpublished name");
    let saved = save(&mut ws, doc, 5);
    assert_eq!(saved["status"], "active");
    assert_eq!(saved["nextScheduledAt"], 60000);
    assert_eq!(saved["hasUnpublishedChanges"], true);
    ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    ws.alarm(60000).unwrap();
    let run = ws.runs[0].view(false).unwrap();
    assert_ne!(run["name"], "Unpublished name");
    assert_eq!(run["revision"], 1);
    assert_eq!(ws.workflows[0].next_scheduled_at, Some(120000));
    assert_eq!(
        ws.handle("POST", "/api/workflows/01-hello/publish", b"{}", 60001, "")
            .unwrap_err()
            .status,
        400
    );
    assert_eq!(ws.workflows[0].graph.revision, 1);
    assert_eq!(ws.revisions.len(), 1);
}
#[test]
fn restore_creates_a_draft_and_republish_creates_monotonic_revision() {
    let mut ws = Workspace::default();
    let doc = save(&mut ws, fixture(), 0);
    let original_name = doc["name"].clone();
    let mut doc = action(&mut ws, "publish", &doc, 1);
    doc["name"] = json!("Second publication");
    let doc = save(&mut ws, doc, 2);
    let doc = action(&mut ws, "publish", &doc, 3);
    let path = "/api/workflows/01-hello/revisions/1/restore";
    assert_eq!(
        ws.handle("POST", path, b"{}", 4, "").unwrap_err().status,
        428
    );
    assert_eq!(
        ws.handle("POST", path, br#"{"expectedVersion":1}"#, 4, "")
            .unwrap_err()
            .status,
        409
    );
    let restored = call(
        &mut ws,
        "POST",
        path,
        json!({"expectedVersion":doc["version"]}),
        4,
    );
    assert_eq!(restored["name"], original_name);
    assert_eq!(restored["revision"], 2);
    assert_eq!(restored["status"], "active");
    assert_eq!(restored["hasUnpublishedChanges"], true);
    assert_eq!(
        ws.live_document("01-hello").unwrap().graph.name,
        "Second publication"
    );
    let published = action(&mut ws, "publish", &restored, 5);
    assert_eq!(published["revision"], 3);
    assert_eq!(
        ws.live_document("01-hello").unwrap().graph.name,
        original_name.as_str().unwrap()
    );
    assert_eq!(ws.revisions.len(), 3);
    assert_eq!(ws.revisions[1].graph.name, "Second publication");
    let history = call(
        &mut ws,
        "GET",
        "/api/workflows/01-hello/revisions",
        Value::Null,
        6,
    );
    assert_eq!(history["revisions"][0]["revision"], 3);
    assert_eq!(history["revisions"][2]["revision"], 1);
    assert_eq!(history["revisions"][0]["publishedBy"], "browser");
}
#[test]
fn legacy_edited_drafts_require_review_without_inventing_history() {
    let mut old = fixture();
    old["revision"] = json!(4);
    old["status"] = json!("draft");
    let mut ws = Workspace::from_json(&json!({"workflows":[old],"runs":[]}).to_string()).unwrap();
    assert!(ws.revisions.is_empty());
    assert!(ws.workflows[0].lifecycle.as_ref().unwrap().needs_review);
    assert_eq!(ws.live_document("01-hello").unwrap_err().status, 409);
    let doc = serde_json::to_value(&ws.workflows[0]).unwrap();
    let doc = action(&mut ws, "publish", &doc, 1);
    assert_eq!(doc["revision"], 5);
    assert_eq!(doc["lifecycle"]["needsReview"], false);
    assert_eq!(ws.revisions.len(), 1);
}
#[test]
fn legacy_active_state_imports_only_a_marked_current_baseline() {
    let mut old = fixture();
    old["revision"] = json!(3);
    old["status"] = json!("active");
    let mut ws = Workspace::from_json(&json!({"workflows":[old],"runs":[]}).to_string()).unwrap();
    assert_eq!(ws.revisions.len(), 1);
    assert!(ws.revisions[0].legacy_baseline);
    assert!(ws.revisions[0].published_at.is_empty());
    assert_eq!(ws.live_document("01-hello").unwrap().graph.revision, 3);
    ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    assert_eq!(ws.revisions.len(), 1);
}
