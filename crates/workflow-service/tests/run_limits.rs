use serde_json::{Value, json};
use workflow_service::{ADMISSION_STATE_BYTES, MAX_STATE_BYTES, Workspace, parse_max_active_runs};

fn workflow(payload: usize) -> Value {
    json!({"schemaVersion":1,"id":"flow","name":"Flow","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Manual Trigger","label":"Start","config":{}},{"id":"map","type":"Map / Transform","label":"Map","config":{"mapping":{"payload":"x".repeat(payload)}}},{"id":"wait","type":"Wait / Delay","label":"Hold","config":{"delayMs":60000}}],"edges":[{"id":"a","source":"start","target":"map"},{"id":"b","source":"map","target":"wait"}]})
}
fn save(ws: &mut Workspace, payload: usize) {
    ws.handle(
        "PUT",
        "/api/workflows/flow",
        &serde_json::to_vec(&workflow(payload)).unwrap(),
        0,
        "",
    )
    .unwrap();
}
fn start(ws: &mut Workspace, id: &str) -> Result<(u16, Value), u16> {
    start_with(ws, id, json!({}))
}
fn start_with(ws: &mut Workspace, id: &str, input: Value) -> Result<(u16, Value), u16> {
    let body =
        serde_json::to_vec(&json!({"workflowId":"flow","mode":"run","input":input})).unwrap();
    ws.handle("POST", "/api/runs", &body, 1, id)
        .map_err(|e| e.status)
}
fn active(ws: &Workspace) -> usize {
    ws.runs
        .iter()
        .filter(|r| r.status().is_ok_and(|s| format!("{s:?}") == "Running"))
        .count()
}

#[test]
fn runs_are_not_limited_by_count_unless_configured() {
    let mut ws = Workspace::default();
    save(&mut ws, 8);
    for i in 0..25 {
        start(&mut ws, &format!("run-{i}")).unwrap();
    }
    assert_eq!(active(&ws), 25, "no count limit by default");
    ws.checkpoint().unwrap();

    let mut ws = Workspace {
        max_active_runs: Some(2),
        ..Workspace::default()
    };
    save(&mut ws, 8);
    start(&mut ws, "a").unwrap();
    start(&mut ws, "b").unwrap();
    assert_eq!(start(&mut ws, "c"), Err(429));
    assert_eq!(active(&ws), 2);
}

#[test]
fn new_runs_stop_before_active_runs_can_exhaust_storage() {
    let mut ws = Workspace::default();
    // Each active run holds a ~60 KB input and a ~40 KB graph (its mapped output
    // repeats the graph's mapping and is stored once), which can't be pruned while
    // it runs.
    save(&mut ws, 40_000);
    let pad = json!({"pad": "p".repeat(60_000)});
    let mut started = 0;
    while start_with(&mut ws, &format!("run-{started}"), pad.clone()).is_ok() {
        started += 1;
        assert!(
            started < 100,
            "admission must stop before the budget is exhausted"
        );
    }
    assert_eq!(start(&mut ws, "refused"), Err(429));
    let text = ws.checkpoint().unwrap();
    assert!(text.len() <= MAX_STATE_BYTES);
    assert!(
        text.len() > ADMISSION_STATE_BYTES - 100_000,
        "admission uses most of the budget"
    );
    // The reserved headroom keeps the workspace writable: active runs can still be
    // cancelled and saved.
    for i in 0..started {
        ws.handle("POST", &format!("/api/runs/run-{i}/cancel"), b"", 2, "")
            .unwrap();
        ws.checkpoint().unwrap();
    }
    assert_eq!(active(&ws), 0);
    assert!(start(&mut ws, "after").is_ok(), "space is admitted again");
}

#[test]
fn max_active_runs_setting_is_parsed_strictly() {
    for unlimited in ["", " ", "0", "unlimited"] {
        assert_eq!(parse_max_active_runs(unlimited), Ok(None), "{unlimited:?}");
    }
    assert_eq!(parse_max_active_runs("5"), Ok(Some(5)));
    assert_eq!(parse_max_active_runs(" 12 "), Ok(Some(12)));
    for invalid in ["-1", "five", "2.5", "1e3"] {
        assert!(parse_max_active_runs(invalid).is_err(), "{invalid:?}");
    }
}
