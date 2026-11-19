use serde_json::{Value, json};
use workflow_service::Workspace;
fn save(ws: &mut Workspace, doc: &Value) {
    ws.handle(
        "PUT",
        &format!("/api/workflows/{}", doc["id"].as_str().unwrap()),
        &serde_json::to_vec(doc).unwrap(),
        0,
        "",
    )
    .unwrap();
}
fn publish(ws: &mut Workspace, id: &str) {
    ws.handle("POST", &format!("/api/workflows/{id}/publish"), b"", 0, "")
        .unwrap();
}
fn child(delay: u64) -> Value {
    json!({"schemaVersion":1,"id":"child","name":"Reusable child","status":"draft","updatedAt":"","nodes":[{"id":"trigger","type":"Execute Workflow Trigger","label":"Invoked","config":{"outputNodeId":"result","inputSchema":{"type":"object"},"outputSchema":{"type":"object","required":["answer"]}}},{"id":"wait","type":"Wait / Delay","label":"Wait","config":{"delayMs":delay}},{"id":"result","type":"Map / Transform","label":"Return result","config":{"mapping":{"answer":42}}}],"edges":[{"id":"a","source":"trigger","target":"wait"},{"id":"b","source":"wait","target":"result"}]})
}
fn parent(mode: &str) -> Value {
    json!({"schemaVersion":1,"id":"parent","name":"Parent","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Manual Trigger","label":"Start","config":{}},{"id":"invoke","type":"Execute Workflow","label":"Call child","config":{"workflowId":"child","mode":mode}}],"edges":[{"id":"a","source":"start","target":"invoke"}]})
}
fn start(ws: &mut Workspace, mode: &str) -> Value {
    ws.handle(
        "POST",
        "/api/runs",
        &serde_json::to_vec(&json!({"workflowId":"parent","mode":mode,"input":{}})).unwrap(),
        0,
        "parent-run",
    )
    .unwrap()
    .1
}
fn get(ws: &mut Workspace, id: &str) -> Value {
    ws.handle("GET", &format!("/api/runs/{id}"), b"", 100, "")
        .unwrap()
        .1
}
#[test]
fn await_pins_child_and_reuses_it_after_restore() {
    let mut ws = Workspace::default();
    save(&mut ws, &child(100));
    publish(&mut ws, "child");
    save(&mut ws, &parent("await"));
    let initial = start(&mut ws, "run");
    assert_eq!(initial["status"], "running");
    let child_id = initial["children"][0]["runId"]
        .as_str()
        .unwrap()
        .to_string();
    let mut changed = child(100);
    changed["nodes"][2]["config"]["mapping"]["answer"] = json!(99);
    save(&mut ws, &changed);
    publish(&mut ws, "child");
    ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    ws.alarm(100).unwrap();
    ws.alarm(101).unwrap();
    let final_run = get(&mut ws, "parent-run");
    assert_eq!(final_run["status"], "succeeded", "{final_run}");
    assert_eq!(
        final_run["nodes"][1]["output"]["output"],
        json!({"answer":42})
    );
    assert_eq!(final_run["children"][0]["revision"], 1);
    assert_eq!(ws.runs.len(), 2);
    assert_eq!(get(&mut ws, &child_id)["parent"]["runId"], "parent-run");
    ws.settle_invocations(200).unwrap();
    assert_eq!(ws.runs.len(), 2);
}
#[test]
fn simulation_propagates_and_background_does_not_block() {
    for (mode, run_mode, expected) in [
        ("await", "simulation", "succeeded"),
        ("background", "run", "succeeded"),
    ] {
        let mut ws = Workspace::default();
        save(&mut ws, &child(100));
        publish(&mut ws, "child");
        save(&mut ws, &parent(mode));
        let run = start(&mut ws, run_mode);
        assert_eq!(run["status"], expected, "{run}");
        let child_id = run["children"][0]["runId"].as_str().unwrap();
        let detail = get(&mut ws, child_id);
        if run_mode == "simulation" {
            assert_eq!(detail["mode"], "simulation");
            assert_eq!(detail["status"], "succeeded")
        } else {
            assert_eq!(detail["status"], "running");
            ws.alarm(100).unwrap();
            assert_eq!(get(&mut ws, child_id)["status"], "succeeded")
        }
    }
}
#[test]
fn cancellation_and_deadlines_cancel_awaited_children() {
    for cancel in [true, false] {
        let mut ws = Workspace::default();
        save(&mut ws, &child(100000));
        publish(&mut ws, "child");
        save(&mut ws, &parent("await"));
        let run = start(&mut ws, "run");
        let id = run["children"][0]["runId"].as_str().unwrap();
        if cancel {
            ws.handle("POST", "/api/runs/parent-run/cancel", b"", 1, "")
                .unwrap();
        } else {
            ws.alarm(30000).unwrap();
        }
        assert_eq!(get(&mut ws, id)["status"], "cancelled");
        assert_eq!(ws.runs.len(), 2);
    }
}
#[test]
fn unavailable_targets_cycles_and_invalid_contracts_fail_closed() {
    let mut ws = Workspace::default();
    save(&mut ws, &parent("await"));
    assert!(
        ws.handle(
            "POST",
            "/api/runs",
            &serde_json::to_vec(&json!({"workflowId":"parent","mode":"run","input":{}})).unwrap(),
            0,
            "missing"
        )
        .is_err()
    );
    assert!(ws.runs.is_empty());
    let mut cyclic = child(0);
    cyclic["nodes"][2] = json!({"id":"result","type":"Execute Workflow","label":"Recursive","config":{"workflowId":"child"}});
    save(&mut ws, &cyclic);
    publish(&mut ws, "child");
    assert!(ws.invocation_targets(&ws.workflows[0].graph).is_err());
    let mut invalid = child(0);
    invalid["nodes"][0]["config"]["inputSchema"] = json!({"type":"object","required":["name"]});
    save(&mut ws, &invalid);
    publish(&mut ws, "child");
    let run = start(&mut ws, "run");
    assert_eq!(run["status"], "failed");
    assert_eq!(run["nodes"][1]["error"], "invocation_failed");
    assert!(run["children"].as_array().unwrap().is_empty());
}

#[test]
fn simulation_data_clock_cannot_expire_an_unrelated_live_parent() {
    let mut ws = Workspace::default();
    save(&mut ws, &child(10000));
    publish(&mut ws, "child");
    save(&mut ws, &parent("await"));
    let live = start(&mut ws, "run");
    assert_eq!(live["status"], "running");
    let mut simulated_child = child(60000);
    simulated_child["id"] = json!("sim-child");
    simulated_child["nodes"][2] = json!({"id":"result","type":"Context Store","label":"Read after virtual wait","config":{"command":{"operation":"get","tableId":"clock-cache","key":"key"}}});
    simulated_child["nodes"][0]["config"]
        .as_object_mut()
        .unwrap()
        .remove("outputSchema");
    save(&mut ws, &simulated_child);
    publish(&mut ws, "sim-child");
    let mut simulated_parent = parent("await");
    simulated_parent["id"] = json!("sim-parent");
    simulated_parent["nodes"][1]["config"]["workflowId"] = json!("sim-child");
    save(&mut ws, &simulated_parent);
    ws.handle(
        "POST",
        "/api/runs",
        &serde_json::to_vec(&json!({"workflowId":"sim-parent","mode":"simulation","input":{}}))
            .unwrap(),
        0,
        "sim-root",
    )
    .unwrap();
    let (id, task, _, time) = ws.pending_data(0).unwrap().unwrap();
    assert_eq!(time, 60000);
    ws.complete_data(
        &id,
        &task.id,
        connector_sdk::Outcome::success(json!({"status":"not_found"})),
        time,
    )
    .unwrap();
    assert_eq!(get(&mut ws, "parent-run")["status"], "running");
    assert_eq!(
        get(&mut ws, live["children"][0]["runId"].as_str().unwrap())["status"],
        "running"
    );
}
#[test]
fn invalid_outputs_fail_children_and_deadlines_cancel_even_when_handled() {
    let mut ws = Workspace::default();
    let mut invalid = child(0);
    invalid["nodes"][0]["config"]["outputSchema"] = json!({"type":"object","required":["missing"]});
    save(&mut ws, &invalid);
    publish(&mut ws, "child");
    save(&mut ws, &parent("await"));
    let run = start(&mut ws, "run");
    assert_eq!(run["status"], "failed");
    assert_eq!(
        get(&mut ws, run["children"][0]["runId"].as_str().unwrap())["status"],
        "failed"
    );
    let mut ws = Workspace::default();
    save(&mut ws, &child(100000));
    publish(&mut ws, "child");
    let mut handled = parent("await");
    handled["nodes"].as_array_mut().unwrap().push(json!({"id":"recover","type":"Map / Transform","label":"Handle timeout","config":{"mapping":{"recovered":true}}}));
    handled["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"error","source":"invoke","target":"recover","sourcePort":"error"}));
    save(&mut ws, &handled);
    let run = start(&mut ws, "run");
    ws.alarm(30000).unwrap();
    assert_eq!(get(&mut ws, "parent-run")["status"], "succeeded");
    assert_eq!(
        get(&mut ws, run["children"][0]["runId"].as_str().unwrap())["status"],
        "cancelled"
    );
}
