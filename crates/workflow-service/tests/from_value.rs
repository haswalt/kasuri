use serde_json::{Value, json};
use workflow_service::Workspace;

#[test]
fn from_value_decodes_exactly_like_from_json() {
    let mut ws = Workspace::default();
    let doc = json!({"schemaVersion":1,"id":"flow","name":"Flow","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Manual Trigger","label":"Start","config":{}},{"id":"wait","type":"Wait / Delay","label":"Wait","config":{"delayMs":50}}],"edges":[{"id":"a","source":"start","target":"wait"}]});
    ws.handle(
        "PUT",
        "/api/workflows/flow",
        &serde_json::to_vec(&doc).unwrap(),
        0,
        "",
    )
    .unwrap();
    ws.handle("POST", "/api/workflows/flow/publish", b"", 0, "")
        .unwrap();
    let body =
        serde_json::to_vec(&json!({"workflowId":"flow","mode":"run","input":{"n":1.5}})).unwrap();
    ws.handle("POST", "/api/runs", &body, 1, "run-1").unwrap();
    let text = ws.checkpoint().unwrap();
    let from_text = Workspace::from_json(&text).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    let from_value = Workspace::from_value(value, text.len()).unwrap();
    assert_eq!(
        from_text.clone().checkpoint().unwrap(),
        from_value.clone().checkpoint().unwrap()
    );
    assert!(
        Workspace::from_value(json!({}), usize::MAX).is_err(),
        "size limit applies"
    );
    // Records stored before run identity was cached are backfilled on this path too.
    let mut legacy: Value = serde_json::from_str(&text).unwrap();
    for run in legacy["runs"].as_array_mut().unwrap() {
        let run = run.as_object_mut().unwrap();
        run.remove("runId");
        run.remove("runStatus");
    }
    let backfilled = Workspace::from_value(legacy, text.len()).unwrap();
    assert!(
        backfilled
            .runs
            .iter()
            .all(|r| !r.run_id.is_empty() && r.run_status.is_some())
    );
    assert!(backfilled.runs.iter().any(|r| r.matches_id("run-1")));
}
