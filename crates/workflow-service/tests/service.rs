use serde_json::{Value, json};
use workflow_service::Workspace;
fn document(trigger: &str) -> Value {
    json!({"schemaVersion":1,"id":"flow","name":"Test","status":"draft","updatedAt":"","nodes":[{"id":"start","type":trigger,"label":"Start","position":{"x":42,"y":24},"config":if trigger=="Schedule" {json!({"cron":"* * * * *","timezone":"UTC"})}else{json!({})}},{"id":"wait","type":"Wait / Delay","label":"Wait","config":{"delayMs":1000}}],"edges":[{"id":"edge","source":"start","target":"wait"}]})
}
fn save(ws: &mut Workspace, doc: &Value) {
    ws.handle(
        "PUT",
        "/api/workflows/flow",
        &serde_json::to_vec(doc).unwrap(),
        0,
        "unused",
    )
    .unwrap();
}
#[test]
fn empty_workspace_has_no_seeds_or_expiry() {
    let value = Workspace::default().snapshot().unwrap();
    assert_eq!(value["workflows"], json!([]));
    assert_eq!(value["runs"], json!([]));
    assert!(value.get("expiresAt").is_none());
}
#[test]
fn real_wait_survives_checkpoint_and_alarm() {
    let mut ws = Workspace::default();
    save(&mut ws, &document("Manual Trigger"));
    let (_, run) = ws
        .handle(
            "POST",
            "/api/runs",
            br#"{"workflowId":"flow","mode":"run","input":{"hello":1}}"#,
            0,
            "run1",
        )
        .unwrap();
    assert_eq!(run["status"], "running");
    assert_eq!(ws.next_alarm(), Some(1000));
    let mut ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    ws.alarm(1000).unwrap();
    let (_, run) = ws.handle("GET", "/api/runs/run1", b"", 1000, "").unwrap();
    assert_eq!(run["status"], "succeeded");
    assert_eq!(ws.next_alarm(), None);
    assert_eq!(
        ws.workflows[0].graph.nodes[0].position.as_ref().unwrap().x,
        42.0
    );
}
#[test]
fn simulation_advances_virtual_time() {
    let mut ws = Workspace::default();
    save(&mut ws, &document("Manual Trigger"));
    let (_, run) = ws
        .handle(
            "POST",
            "/api/runs",
            br#"{"workflowId":"flow","mode":"simulation","input":{}}"#,
            0,
            "sim",
        )
        .unwrap();
    assert_eq!(run["status"], "succeeded");
    assert_eq!(ws.next_alarm(), None);
}
#[test]
fn publishing_registers_cron_and_pause_removes_it() {
    let mut ws = Workspace::default();
    save(&mut ws, &document("Schedule"));
    ws.handle("POST", "/api/workflows/flow/publish", b"", 0, "")
        .unwrap();
    assert_eq!(ws.next_alarm(), Some(60000));
    ws.alarm(60000).unwrap();
    assert_eq!(ws.runs.len(), 1);
    assert_eq!(ws.runs[0].view(false).unwrap()["mode"], "schedule");
    ws.alarm(60000).unwrap();
    assert_eq!(ws.runs.len(), 1);
    ws.handle("POST", "/api/workflows/flow/pause", b"", 60000, "")
        .unwrap();
    assert!(ws.workflows[0].next_scheduled_at.is_none());
}
#[test]
fn invalid_cron_is_rejected_without_publishing() {
    let mut ws = Workspace::default();
    let mut doc = document("Schedule");
    doc["nodes"][0]["config"]["cron"] = json!("not cron");
    save(&mut ws, &doc);
    assert_eq!(
        ws.handle("POST", "/api/workflows/flow/publish", b"", 0, "")
            .unwrap_err()
            .status,
        400
    );
    assert_eq!(ws.snapshot().unwrap()["workflows"][0]["status"], "draft");
}
