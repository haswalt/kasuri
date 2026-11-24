//! Run state is stored with repeated values once, without changing what the API
//! shows.
use serde_json::{Value, json};
use workflow_service::Workspace;

fn hold() -> Value {
    json!({"schemaVersion":1,"id":"hold","name":"Hold","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Manual Trigger","label":"Start","config":{}},{"id":"wait","type":"Wait / Delay","label":"Hold","config":{"delayMs":60000}}],"edges":[{"id":"e","source":"start","target":"wait"}]})
}

#[test]
fn passed_through_input_is_stored_once() {
    let mut ws = Workspace::default();
    ws.handle(
        "PUT",
        "/api/workflows/hold",
        hold().to_string().as_bytes(),
        0,
        "",
    )
    .unwrap();
    let payload = "Q".repeat(50_000);
    let body = json!({"workflowId":"hold","mode":"run","input":{"payload":payload}});
    ws.handle("POST", "/api/runs", body.to_string().as_bytes(), 1, "r1")
        .unwrap();
    // The input, the trigger's output and the waiting node's output are the same
    // value: stored once.
    let stored = serde_json::to_string(&ws.runs[0]).unwrap();
    assert_eq!(stored.matches(&payload).count(), 1);
    assert!(stored.len() < 60_000, "{}", stored.len());
    // The API still shows every copy.
    let (_, run) = ws.handle("GET", "/api/runs/r1", b"", 2, "").unwrap();
    assert_eq!(run["input"]["payload"], payload.as_str());
    let start = run["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "start")
        .unwrap();
    assert_eq!(start["output"]["payload"], payload.as_str());
    // The run still advances, restored from the compact state.
    ws.alarm(60_001).unwrap();
    let (_, run) = ws.handle("GET", "/api/runs/r1", b"", 60_002, "").unwrap();
    assert_eq!(run["status"], "succeeded");
}

#[test]
fn plain_checkpoints_from_earlier_releases_still_restore() {
    let mut ws = Workspace::default();
    ws.handle(
        "PUT",
        "/api/workflows/hold",
        hold().to_string().as_bytes(),
        0,
        "",
    )
    .unwrap();
    let payload = "P".repeat(5_000);
    let body = json!({"workflowId":"hold","mode":"run","input":{"payload":payload}});
    ws.handle("POST", "/api/runs", body.to_string().as_bytes(), 1, "r1")
        .unwrap();
    // Rewrite the run's checkpoint in the plain form earlier releases stored.
    let plain = workflow_service::checkpoint_state(&ws.runs[0].checkpoint)
        .unwrap()
        .to_string();
    assert!(plain.matches(&payload).count() >= 2);
    let mut stored = serde_json::to_value(&ws).unwrap();
    stored["runs"][0]["checkpoint"] = json!(plain);
    let mut ws = Workspace::from_json(&stored.to_string()).unwrap();
    let (_, run) = ws.handle("GET", "/api/runs/r1", b"", 2, "").unwrap();
    assert_eq!(run["input"]["payload"], payload.as_str());
    ws.alarm(60_001).unwrap();
    assert_eq!(
        ws.handle("GET", "/api/runs/r1", b"", 60_002, "").unwrap().1["status"],
        "succeeded"
    );
    // Once the run is written again, it is stored compactly.
    assert_eq!(
        serde_json::to_string(&ws.runs[0])
            .unwrap()
            .matches(&payload)
            .count(),
        1
    );
}
