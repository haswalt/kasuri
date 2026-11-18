use serde_json::{Value, json};
use workflow_service::Workspace;
fn document() -> Value {
    serde_json::from_str(include_str!("../../../examples/workflows/01-hello.json")).unwrap()
}
#[test]
fn explicit_edit_versions_prevent_lost_updates_and_track_revision_references() {
    let mut ws = Workspace::default();
    let (_, mut doc) = ws
        .handle(
            "PUT",
            "/api/workflows/01-hello",
            &serde_json::to_vec(&document()).unwrap(),
            0,
            "create",
        )
        .unwrap();
    assert_eq!(doc["version"], 1);
    let stale = doc.clone();
    doc["name"] = json!("Updated");
    let (_, saved) = ws
        .handle(
            "PUT",
            "/api/workflows/01-hello",
            &serde_json::to_vec(&doc).unwrap(),
            1,
            "edit",
        )
        .unwrap();
    assert_eq!(saved["version"], 2);
    assert_eq!(
        ws.handle(
            "PUT",
            "/api/workflows/01-hello",
            &serde_json::to_vec(&stale).unwrap(),
            2,
            "stale"
        )
        .unwrap_err()
        .status,
        409
    );
    assert_eq!(ws.audit[1]["before"]["version"], 1);
    assert_eq!(ws.audit[1]["after"]["version"], 2);
    assert_eq!(ws.audit[2]["outcome"], "rejected");
    assert_eq!(
        ws.handle(
            "POST",
            "/api/runs",
            &serde_json::to_vec(
                &json!({"workflowId":"01-hello","mode":"simulation","input":{},"expectedVersion":1})
            )
            .unwrap(),
            3,
            "stale-run"
        )
        .unwrap_err()
        .status,
        409
    );
    assert_eq!(
        ws.handle(
            "POST",
            "/api/workflows/01-hello/publish",
            br#"{"expectedVersion":1}"#,
            3,
            "stale-publish"
        )
        .unwrap_err()
        .status,
        409
    );
    assert_eq!(ws.runs.len(), 0);
}
#[test]
fn private_management_storage_survives_checkpoint_but_never_enters_snapshots() {
    let mut ws = Workspace::default();
    ws.management = json!({"privateMarker":"not-an-api-field"});
    ws.record_audit(
        "agent-id",
        "workflows.create",
        "01-hello",
        "succeeded",
        0,
        "request-id",
    );
    let restored = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    assert_eq!(restored.management, ws.management);
    assert!(
        !restored
            .snapshot()
            .unwrap()
            .to_string()
            .contains("privateMarker")
    );
    assert_eq!(
        restored.audit_page(0, 25)["items"][0]["correlationId"],
        "request-id"
    );
}
