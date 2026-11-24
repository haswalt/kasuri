use serde_json::{Value, json};
use workflow_service::{MAX_STATE_BYTES, Workspace};

fn workflow(payload: usize) -> Value {
    json!({"schemaVersion":1,"id":"flow","name":"Flow","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Manual Trigger","label":"Start","config":{}},{"id":"map","type":"Map / Transform","label":"Map","config":{"mapping":{"payload":"x".repeat(payload)}}}],"edges":[{"id":"a","source":"start","target":"map"}]})
}
fn run(ws: &mut Workspace, id: &str, now: u64) {
    let body = serde_json::to_vec(&json!({"workflowId":"flow","mode":"run","input":{}})).unwrap();
    let (status, value) = ws.handle("POST", "/api/runs", &body, now, id).unwrap();
    assert!(status < 300, "{value}");
}
fn ids(ws: &Workspace) -> Vec<String> {
    ws.runs.iter().map(|r| r.id_string().unwrap()).collect()
}

#[test]
fn run_count_retention_keeps_the_newest_hundred() {
    let mut ws = Workspace::default();
    ws.handle(
        "PUT",
        "/api/workflows/flow",
        &serde_json::to_vec(&workflow(8)).unwrap(),
        0,
        "",
    )
    .unwrap();
    for i in 0..130 {
        run(&mut ws, &format!("run-{i:03}"), i);
        ws.checkpoint().unwrap();
    }
    assert_eq!(ws.runs.len(), 100);
    let expected: Vec<String> = (30..130).rev().map(|i| format!("run-{i:03}")).collect();
    assert_eq!(ids(&ws), expected);
}

#[test]
fn byte_retention_removes_only_as_many_old_runs_as_needed() {
    let mut ws = Workspace::default();
    // Each run stores a ~20 KiB output; 90 of them exceed the 1.5 MB budget.
    ws.handle(
        "PUT",
        "/api/workflows/flow",
        &serde_json::to_vec(&workflow(20_000)).unwrap(),
        0,
        "",
    )
    .unwrap();
    for i in 0..90 {
        run(&mut ws, &format!("run-{i:03}"), i);
    }
    let before = ws.runs.clone();
    let text = ws.checkpoint().unwrap();
    assert!(text.len() <= MAX_STATE_BYTES);
    let kept = ws.runs.len();
    assert!(kept < 90 && kept > 1);
    // The newest runs are kept, in order.
    assert_eq!(
        ids(&ws),
        before[..kept]
            .iter()
            .map(|r| r.id_string().unwrap())
            .collect::<Vec<_>>()
    );
    // Minimal: keeping the next-oldest run as well would exceed the budget.
    let next = serde_json::to_string(&before[kept]).unwrap().len() + 1;
    assert!(text.len() + next > MAX_STATE_BYTES);
}

#[test]
fn audit_retention_trims_oldest_entries_to_fit() {
    let mut ws = Workspace::default();
    for i in 0..400 {
        ws.audit.push(json!({"id":i,"detail":"y".repeat(5_000)}));
    }
    let text = ws.checkpoint().unwrap();
    assert!(text.len() <= MAX_STATE_BYTES);
    assert_eq!(ws.audit.last().unwrap()["id"], 399, "newest entry kept");
    let first = ws.audit[0]["id"].as_u64().unwrap();
    assert!(first > 0);
    // Minimal: restoring the newest removed entry would exceed the budget.
    let restored =
        serde_json::to_string(&json!({"id":first - 1,"detail":"y".repeat(5_000)})).unwrap();
    assert!(text.len() + restored.len() + 1 > MAX_STATE_BYTES);
}
