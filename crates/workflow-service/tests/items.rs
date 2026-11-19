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
fn setup(delay: u64, policy: &str, concurrency: usize) -> Workspace {
    let mut ws = Workspace::default();
    let child = json!({"schemaVersion":1,"id":"item-child","name":"Item child","description":"","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Execute Workflow Trigger","label":"Each item","config":{"outputNodeId":"output"}},{"id":"wait","type":"Wait / Delay","label":"Wait","config":{"delayMs":delay}},{"id":"output","type":"Map / Transform","label":"Return","config":{"mapping":{"value":{"$input":""}}}}],"edges":[{"id":"a","source":"start","target":"wait"},{"id":"b","source":"wait","target":"output"}]});
    save(&mut ws, &child);
    ws.handle("POST", "/api/workflows/item-child/publish", b"", 0, "")
        .unwrap();
    let parent = json!({"schemaVersion":1,"id":"parent","name":"Each parent","description":"","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Manual Trigger","label":"Start"},{"id":"each","type":"Each / Loop","label":"Each","config":{"workflowId":"item-child","concurrency":concurrency,"failurePolicy":policy}}],"edges":[{"id":"e","source":"start","target":"each"}]});
    save(&mut ws, &parent);
    ws
}
fn start(ws: &mut Workspace, input: Value, mode: &str) -> Value {
    ws.handle(
        "POST",
        "/api/runs",
        &serde_json::to_vec(&json!({"workflowId":"parent","input":input,"mode":mode})).unwrap(),
        0,
        "root",
    )
    .unwrap()
    .1
}
fn get(ws: &mut Workspace) -> Value {
    ws.handle("GET", "/api/runs/root", b"", 0, "").unwrap().1
}
#[test]
fn items_restore_in_order_without_repeating_completed_children() {
    let mut ws = setup(100, "collect", 2);
    let initial = start(&mut ws, json!([3, 1, 2]), "run");
    assert_eq!(initial["children"].as_array().unwrap().len(), 2);
    let mut changed = serde_json::to_value(
        ws.workflows
            .iter()
            .find(|w| w.graph.id == "item-child")
            .unwrap(),
    )
    .unwrap();
    changed["nodes"][2]["config"]["mapping"]["value"] = json!("new publication");
    save(&mut ws, &changed);
    ws.handle("POST", "/api/workflows/item-child/publish", b"", 0, "")
        .unwrap();
    ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    ws.alarm(100).unwrap();
    ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    ws.alarm(200).unwrap();
    let result = get(&mut ws);
    assert_eq!(result["status"], "succeeded", "{result}");
    let items = &result["nodes"][1]["output"]["items"];
    assert_eq!(items[0]["output"], json!({"value":3}));
    assert_eq!(items[1]["output"], json!({"value":1}));
    assert_eq!(items[2]["output"], json!({"value":2}));
    assert_eq!(result["children"].as_array().unwrap().len(), 3);
    assert_ne!(items[0]["itemId"], items[1]["itemId"]);
    ws.settle_invocations(300).unwrap();
    assert_eq!(ws.runs.len(), 4);
}
#[test]
fn empty_large_and_simulated_collections_are_explicit() {
    let mut ws = setup(100, "collect", 4);
    let run = start(&mut ws, json!([]), "simulation");
    assert_eq!(run["nodes"][1]["output"]["total"], 0);
    assert!(run["children"].as_array().unwrap().is_empty());
    let mut ws = setup(100, "collect", 4);
    let mut run = start(&mut ws, json!((0..100).collect::<Vec<_>>()), "simulation");
    for _ in 0..100 {
        if run["status"] != "running" {
            break;
        }
        let at = ws.next_alarm().unwrap();
        ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
        ws.alarm(at).unwrap();
        run = get(&mut ws);
    }
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["nodes"][1]["output"]["succeeded"], 100);
    assert!(
        ws.runs
            .iter()
            .all(|r| r.mode == workflow_service::Mode::Simulation)
    );
    assert!(ws.runs.len() <= 100);
}
#[test]
fn item_failure_policy_and_cancellation_preserve_successful_work() {
    for policy in ["fail", "collect"] {
        let mut ws = setup(0, policy, 2);
        let mut child = serde_json::to_value(&ws.workflows[0]).unwrap();
        child["nodes"][0]["config"]["inputSchema"] = json!({"type":"number"});
        save(&mut ws, &child);
        ws.handle("POST", "/api/workflows/item-child/publish", b"", 0, "")
            .unwrap();
        let run = start(&mut ws, json!([1, "invalid", 3]), "run");
        assert_eq!(
            run["status"],
            if policy == "collect" {
                "succeeded"
            } else {
                "failed"
            },
            "{run}"
        );
        if policy == "collect" {
            assert_eq!(run["nodes"][1]["output"]["failed"], 1);
            assert_eq!(run["nodes"][1]["output"]["succeeded"], 2)
        }
    }
    let mut ws = setup(10000, "collect", 2);
    let run = start(&mut ws, json!([1, 2, 3]), "run");
    ws.handle("POST", "/api/runs/root/cancel", b"", 1, "")
        .unwrap();
    assert!(
        ws.runs
            .iter()
            .all(|r| r.status().unwrap() == workflow_engine::RunStatus::Cancelled)
    );
    assert!(
        get(&mut ws)["iterations"]["each"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["status"] == "cancelled")
    );
    assert_eq!(run["children"].as_array().unwrap().len(), 2);
}

#[test]
fn retries_are_local_to_the_failed_item_and_keep_effect_identity() {
    use connector_sdk::{Failure, FailureCode, Outcome};
    let mut ws = setup(0, "collect", 2);
    let mut child = serde_json::to_value(&ws.workflows[0]).unwrap();
    child["nodes"] = json!([{"id":"start","type":"Execute Workflow Trigger","label":"Item","config":{"outputNodeId":"http"}},{"id":"http","type":"HTTP Request","label":"Fetch","retry":{"maxAttempts":2,"delayMs":10,"maxDelayMs":10},"config":{"url":"https://fixture.example/item"}}]);
    child["edges"] = json!([{"id":"e","source":"start","target":"http"}]);
    save(&mut ws, &child);
    ws.handle("POST", "/api/workflows/item-child/publish", b"", 0, "")
        .unwrap();
    start(&mut ws, json!([1, 2]), "run");
    let first = ws.claim_external(0).unwrap().unwrap();
    let second = ws.claim_external(0).unwrap().unwrap();
    ws.complete_external(
        &first.0,
        &first.1.id,
        Outcome::Failed {
            failure: Failure {
                code: FailureCode::External,
                retryable: true,
            },
        },
        1,
    )
    .unwrap();
    ws.complete_external(
        &second.0,
        &second.1.id,
        Outcome::success(json!({"done":"second"})),
        2,
    )
    .unwrap();
    ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    ws.alarm(11).unwrap();
    let retry = ws.claim_external(11).unwrap().unwrap();
    assert_eq!(retry.0, first.0);
    assert_eq!(retry.1.attempt, 2);
    assert_eq!(retry.1.idempotency_key, first.1.idempotency_key);
    assert!(ws.claim_external(11).unwrap().is_none());
    ws.complete_external(
        &retry.0,
        &retry.1.id,
        Outcome::success(json!({"done":"retried"})),
        12,
    )
    .unwrap();
    let run = get(&mut ws);
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["children"].as_array().unwrap().len(), 2);
    assert!(
        run["nodes"][1]["output"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["output"] == json!({"done":"second"}))
    );
}
#[test]
fn paused_parent_keeps_item_outputs_and_does_not_launch_more_items() {
    let mut ws = setup(100, "collect", 2);
    start(&mut ws, json!([1, 2, 3]), "run");
    ws.handle("POST", "/api/runs/root/pause", b"", 1, "")
        .unwrap();
    ws.alarm(100).unwrap();
    assert_eq!(get(&mut ws)["children"].as_array().unwrap().len(), 2);
    ws.handle("POST", "/api/runs/root/resume", b"", 101, "")
        .unwrap();
    ws.alarm(201).unwrap();
    assert_eq!(get(&mut ws)["status"], "succeeded");
}

#[test]
fn nested_each_reserves_capacity_for_leaf_work() {
    let mut ws = setup(50, "collect", 4);
    let nested = json!({"schemaVersion":1,"id":"nested","name":"Nested each","description":"","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Execute Workflow Trigger","label":"Batch","config":{"outputNodeId":"each"}},{"id":"each","type":"Each / Loop","label":"Inner items","config":{"workflowId":"item-child","concurrency":4,"failurePolicy":"collect"}}],"edges":[{"id":"e","source":"start","target":"each"}]});
    save(&mut ws, &nested);
    ws.handle("POST", "/api/workflows/nested/publish", b"", 0, "")
        .unwrap();
    let mut parent = serde_json::to_value(
        ws.workflows
            .iter()
            .find(|w| w.graph.id == "parent")
            .unwrap(),
    )
    .unwrap();
    parent["nodes"][1]["config"]["workflowId"] = json!("nested");
    save(&mut ws, &parent);
    let run = start(&mut ws, json!([[1, 2], [3, 4]]), "simulation");
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["nodes"][1]["output"]["succeeded"], 2);
    assert_eq!(
        run["nodes"][1]["output"]["items"][0]["output"]["succeeded"],
        2
    );
}
