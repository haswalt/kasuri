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

fn http_workspace(method: &str) -> Workspace {
    let mut ws = Workspace::default();
    let mut doc = document("Manual Trigger");
    doc["nodes"][1]["type"] = json!("HTTP Request");
    doc["nodes"][1]["config"] = json!({"url":"https://api.example.com/items","method":method,"simulationResponse":{"status":200,"body":{"ok":true}}});
    save(&mut ws, &doc);
    ws.handle(
        "POST",
        "/api/runs",
        br#"{"workflowId":"flow","mode":"run","input":{}}"#,
        0,
        "http-run",
    )
    .unwrap();
    ws
}
#[test]
fn http_dispatch_is_checkpointed_claimed_once_and_completes_after_restart() {
    let mut ws = http_workspace("GET");
    assert_eq!(ws.runs[0].view(false).unwrap()["status"], "running");
    let (id, task) = ws.claim_external(1).unwrap().unwrap();
    assert!(ws.claim_external(1).unwrap().is_none());
    let mut ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    assert!(ws.claim_external(2).unwrap().is_none());
    assert!(
        ws.complete_external(
            &id,
            &task.id,
            connector_sdk::Outcome::success(json!({"body":[]})),
            2
        )
        .unwrap()
    );
    assert_eq!(ws.runs[0].view(false).unwrap()["status"], "succeeded");
    assert!(
        !ws.complete_external(&id, &task.id, connector_sdk::Outcome::success(json!({})), 3)
            .unwrap()
    );
}
#[test]
fn cancel_and_pause_are_preserved_when_http_result_arrives() {
    for action in ["pause", "cancel"] {
        let mut ws = http_workspace("POST");
        let (id, task) = ws.claim_external(1).unwrap().unwrap();
        ws.handle("POST", &format!("/api/runs/http-run/{action}"), b"", 2, "")
            .unwrap();
        ws.complete_external(&id, &task.id, connector_sdk::Outcome::success(json!({})), 3)
            .unwrap();
        assert_eq!(
            ws.runs[0].view(false).unwrap()["status"],
            if action == "pause" {
                "paused"
            } else {
                "cancelled"
            }
        );
    }
}
#[test]
fn recovered_write_with_unknown_outcome_is_not_dispatched_again() {
    let mut ws = http_workspace("POST");
    let (_, task) = ws.claim_external(1).unwrap().unwrap();
    let mut ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    ws.alarm(task.deadline_ms).unwrap();
    assert!(ws.claim_external(task.deadline_ms).unwrap().is_none());
    assert_eq!(ws.runs[0].view(false).unwrap()["status"], "failed");
}
#[test]
fn http_simulation_never_creates_an_external_dispatch() {
    let mut ws = http_workspace("GET");
    let (_, run) = ws
        .handle(
            "POST",
            "/api/runs",
            br#"{"workflowId":"flow","mode":"simulation","input":{}}"#,
            1,
            "http-sim",
        )
        .unwrap();
    assert_eq!(run["status"], "succeeded");
    assert!(ws.runs[0].external.is_none());
    assert_eq!(run["nodes"][1]["output"]["body"]["ok"], true);
}

#[test]
fn webhook_events_are_durable_deduplicated_and_disabled_by_pause_or_rotation() {
    let mut ws = Workspace::default();
    save(&mut ws, &document("Webhook"));
    ws.configure_webhook("flow", "endpoint".into(), "hash".into())
        .unwrap();
    assert!(
        ws.receive_webhook("flow", "endpoint", "event1", json!({}), "digest", 0, "run1")
            .is_err()
    );
    ws.handle("POST", "/api/workflows/flow/publish", b"", 0, "")
        .unwrap();
    let first = ws
        .receive_webhook(
            "flow",
            "endpoint",
            "event1",
            json!({"hello":1}),
            "digest",
            1,
            "run1",
        )
        .unwrap();
    assert_eq!(first["duplicate"], false);
    let mut ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    let repeat = ws
        .receive_webhook(
            "flow",
            "endpoint",
            "event1",
            json!({"hello":1}),
            "digest",
            2,
            "run2",
        )
        .unwrap();
    assert_eq!(repeat["runId"], "run1");
    assert_eq!(repeat["duplicate"], true);
    assert_eq!(ws.runs.len(), 1);
    assert_eq!(
        ws.receive_webhook(
            "flow",
            "endpoint",
            "event1",
            json!({}),
            "different",
            3,
            "run3"
        )
        .unwrap_err()
        .status,
        409
    );
    assert!(ws.snapshot().unwrap().get("webhooks").is_none());
    ws.handle("POST", "/api/workflows/flow/pause", b"", 4, "")
        .unwrap();
    assert_eq!(
        ws.receive_webhook("flow", "endpoint", "event2", json!({}), "digest", 5, "run2")
            .unwrap_err()
            .status,
        404
    );
    ws.handle("POST", "/api/workflows/flow/resume", b"", 6, "")
        .unwrap();
    ws.configure_webhook("flow", "new-endpoint".into(), "new-hash".into())
        .unwrap();
    assert_eq!(
        ws.receive_webhook("flow", "endpoint", "event2", json!({}), "digest", 7, "run2")
            .unwrap_err()
            .status,
        404
    );
}

#[test]
fn pending_data_is_not_uncertain_network_io_and_simulation_clock_survives_restore() {
    let mut ws = Workspace::default();
    let mut doc = document("Manual Trigger");
    doc["nodes"].as_array_mut().unwrap().push(json!({"id":"data","type":"Context Store","label":"Data","config":{"command":{"operation":"get","tableId":"cache","key":"x"}}}));
    doc["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"data-edge","source":"wait","target":"data"}));
    save(&mut ws, &doc);
    ws.handle(
        "POST",
        "/api/runs",
        br#"{"workflowId":"flow","mode":"simulation","input":{}}"#,
        10,
        "data-sim",
    )
    .unwrap();
    let mut ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    // Recovery wall time cannot expire a task queued after a virtual wait.
    ws.alarm(1_000_000).unwrap();
    assert!(ws.claim_external(1_000_000).unwrap().is_none());
    let (id, task, mode, time) = ws.pending_data(1_000_000).unwrap().unwrap();
    assert_eq!(mode, workflow_service::Mode::Simulation);
    assert_eq!(time, 1010);
    assert!(
        ws.complete_data(
            &id,
            &task.id,
            connector_sdk::Outcome::success(json!({"status":"not_found"})),
            time
        )
        .unwrap()
    );
    assert_eq!(ws.runs[0].view(false).unwrap()["status"], "succeeded");
    assert!(ws.pending_data(1_000_000).unwrap().is_none());

    ws.handle(
        "POST",
        "/api/runs",
        br#"{"workflowId":"flow","mode":"simulation","input":{}}"#,
        20,
        "cancel-data",
    )
    .unwrap();
    ws.handle("POST", "/api/runs/cancel-data/pause", b"", 20, "")
        .unwrap();
    assert!(ws.pending_data(20).unwrap().is_none());
    assert_eq!(
        ws.data_runs().unwrap().len(),
        1,
        "Paused simulation retains its snapshot"
    );
    ws.handle("POST", "/api/runs/cancel-data/cancel", b"", 20, "")
        .unwrap();
    assert!(ws.pending_data(20).unwrap().is_none());
    assert!(
        ws.data_runs().unwrap().is_empty(),
        "Terminal simulations release their snapshot"
    );
}
