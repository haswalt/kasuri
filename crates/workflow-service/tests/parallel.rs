use connector_sdk::Outcome;
use serde_json::{Value, json};
use workflow_service::Workspace;
fn graph() -> Value {
    json!({"schemaVersion":1,"id":"parallel","name":"Parallel","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Manual Trigger","label":"Start"},{"id":"split","type":"Split / Parallel","label":"Parallel","config":{"concurrency":2}},{"id":"a","type":"HTTP Request","label":"A","config":{"url":"https://fixture.example/a"}},{"id":"b","type":"HTTP Request","label":"B","config":{"url":"https://fixture.example/b"}},{"id":"join","type":"Join","label":"Join","config":{"policy":"all"}}],"edges":[{"id":"s","source":"start","target":"split"},{"id":"a","source":"split","target":"a"},{"id":"b","source":"split","target":"b"},{"id":"aj","source":"a","target":"join"},{"id":"bj","source":"b","target":"join"}]})
}
fn prepare() -> Workspace {
    let mut ws = Workspace::default();
    ws.handle(
        "PUT",
        "/api/workflows/parallel",
        &serde_json::to_vec(&graph()).unwrap(),
        0,
        "",
    )
    .unwrap();
    ws.handle(
        "POST",
        "/api/runs",
        br#"{"workflowId":"parallel","mode":"run","input":{}}"#,
        0,
        "run",
    )
    .unwrap();
    ws
}
#[test]
fn in_flight_set_merges_outcomes_and_survives_restore() {
    let mut ws = prepare();
    assert_eq!(ws.runs[0].pending.len(), 2);
    let (id, a) = ws.claim_external(0).unwrap().unwrap();
    let (_, b) = ws.claim_external(0).unwrap().unwrap();
    assert!(ws.claim_external(0).unwrap().is_none());
    ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    assert!(ws.claim_external(1).unwrap().is_none());
    assert!(
        ws.complete_external(&id, &b.id, Outcome::success(json!(2)), 1)
            .unwrap()
    );
    assert_eq!(ws.runs[0].pending.len(), 1);
    assert!(
        ws.complete_external(&id, &a.id, Outcome::success(json!(1)), 2)
            .unwrap()
    );
    let run = ws.handle("GET", "/api/runs/run", b"", 2, "").unwrap().1;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["nodes"][4]["output"], json!({"a":1,"b":2}));
    assert!(
        !ws.complete_external(&id, &b.id, Outcome::success(json!(9)), 3)
            .unwrap()
    );
}
#[test]
fn cancellation_rejects_every_late_sibling_completion() {
    let mut ws = prepare();
    let (id, a) = ws.claim_external(0).unwrap().unwrap();
    let (_, b) = ws.claim_external(0).unwrap().unwrap();
    ws.handle("POST", "/api/runs/run/cancel", b"", 1, "")
        .unwrap();
    for task in [a, b] {
        assert!(
            !ws.complete_external(&id, &task.id, Outcome::success(json!(true)), 2)
                .unwrap()
        );
    }
    assert!(ws.runs[0].pending.is_empty());
}
#[test]
fn legacy_single_external_slot_still_restores() {
    let mut doc = graph();
    doc["nodes"][1]["type"] = json!("Merge");
    doc["nodes"][1]["config"] = json!({});
    let mut ws = Workspace::default();
    ws.handle(
        "PUT",
        "/api/workflows/parallel",
        &serde_json::to_vec(&doc).unwrap(),
        0,
        "",
    )
    .unwrap();
    ws.handle(
        "POST",
        "/api/runs",
        br#"{"workflowId":"parallel","mode":"run","input":{}}"#,
        0,
        "legacy",
    )
    .unwrap();
    assert!(ws.runs[0].external.is_some());
    assert!(ws.runs[0].pending.is_empty());
    ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    let (id, task) = ws.claim_external(0).unwrap().unwrap();
    ws.complete_external(&id, &task.id, Outcome::success(Value::Null), 1)
        .unwrap();
    assert!(ws.runs[0].external.is_some());
    assert!(ws.runs[0].pending.is_empty());
}

#[test]
fn claiming_http_preserves_an_earlier_wait_and_response_deadline() {
    let mut graph = graph();
    graph["nodes"][3] =
        json!({"id":"b","type":"Wait / Delay","label":"Short wait","config":{"delayMs":100}});
    let mut ws = Workspace::default();
    ws.handle(
        "PUT",
        "/api/workflows/parallel",
        &serde_json::to_vec(&graph).unwrap(),
        0,
        "",
    )
    .unwrap();
    ws.handle(
        "POST",
        "/api/runs",
        br#"{"workflowId":"parallel","mode":"run","input":{}}"#,
        0,
        "run",
    )
    .unwrap();
    ws.claim_external(0).unwrap().unwrap();
    assert!(ws.next_alarm().unwrap() <= 100);
    ws.alarm(1).unwrap();
    assert_eq!(ws.next_alarm(), Some(100));
}
