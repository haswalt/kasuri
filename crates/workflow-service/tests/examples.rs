//! The published examples are the test fixtures: do not copy their graphs here.
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::PathBuf};
use workflow_service::{Workspace, manifests};

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}
fn cases() -> Vec<Value> {
    serde_json::from_str(&fs::read_to_string(examples().join("scenarios.json")).unwrap()).unwrap()
}
fn load(case: &Value) -> Value {
    serde_json::from_str(
        &fs::read_to_string(examples().join(case["file"].as_str().unwrap())).unwrap(),
    )
    .unwrap()
}
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
fn check(case: &Value, run: &Value) {
    assert_eq!(run["status"], "succeeded", "{}: {run}", case["file"]);
    for (id, expected) in case["outputs"].as_object().unwrap() {
        let node = run["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == *id)
            .unwrap();
        assert_eq!(&node["output"], expected, "{} node {id}", case["file"]);
    }
    for (id, expected) in case["statuses"].as_object().unwrap() {
        let node = run["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == *id)
            .unwrap();
        assert_eq!(&node["status"], expected, "{} node {id}", case["file"]);
    }
}
#[test]
fn every_published_example_simulates_without_input_or_configuration() {
    let mut covered = BTreeSet::new();
    let mut listed = BTreeSet::new();
    for case in cases() {
        listed.insert(case["file"].as_str().unwrap().to_owned());
        let doc = load(&case);
        assert_eq!(doc["status"], "draft");
        assert_eq!(doc["revision"], 0);
        let graph = workflow_schema::Workflow::from_json(&doc.to_string()).unwrap();
        for node in &graph.nodes {
            covered.insert(node.kind.clone());
            assert_eq!(node.version, 1);
            assert!(
                !node.config.contains_key("passwordSecret"),
                "Examples must not need installation secrets"
            );
        }
        let mut ws = Workspace::default();
        save(&mut ws, &doc);
        // Publishing validates configuration, including cron/webhook, without enabling
        // anything on a real installation: this workspace exists only in this test.
        ws.handle(
            "POST",
            &format!("/api/workflows/{}/publish", graph.id),
            b"",
            0,
            "",
        )
        .unwrap();
        let (_, run) = ws
            .handle(
                "POST",
                "/api/runs",
                &serde_json::to_vec(&json!({"workflowId":graph.id,"mode":"simulation","input":{}}))
                    .unwrap(),
                0,
                "example-run",
            )
            .unwrap();
        check(&case, &run);
        // Core examples also execute real waits and survive checkpoint restoration.
        if !graph.nodes.iter().any(|n| n.kind == "HTTP Request") {
            let mut ws = Workspace::default();
            save(&mut ws, &doc);
            let (_, mut run) = ws
                .handle(
                    "POST",
                    "/api/runs",
                    &serde_json::to_vec(&json!({"workflowId":graph.id,"mode":"run","input":{}}))
                        .unwrap(),
                    0,
                    "real-run",
                )
                .unwrap();
            for _ in 0..20 {
                if run["status"] != "running" {
                    break;
                }
                let at = ws
                    .next_alarm()
                    .expect("Durable wait must schedule an alarm");
                ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
                ws.alarm(at).unwrap();
                run = ws
                    .handle("GET", "/api/runs/real-run", b"", at, "")
                    .unwrap()
                    .1;
            }
            check(&case, &run);
        }
    }
    let files: BTreeSet<_> = fs::read_dir(examples().join("workflows"))
        .unwrap()
        .map(|entry| format!("workflows/{}", entry.unwrap().file_name().to_str().unwrap()))
        .collect();
    assert_eq!(
        listed, files,
        "Every published workflow needs an executable scenario"
    );
    let supported: BTreeSet<_> = manifests().into_iter().map(|m| m.kind).collect();
    assert_eq!(
        covered, supported,
        "Add a working example and assertion when adding a connector"
    );
}
#[test]
fn scheduled_example_runs_from_its_actual_trigger() {
    let case = cases()
        .into_iter()
        .find(|c| c["file"] == "workflows/08-schedule.json")
        .unwrap();
    let doc = load(&case);
    let mut ws = Workspace::default();
    save(&mut ws, &doc);
    ws.handle("POST", "/api/workflows/08-schedule/publish", b"", 0, "")
        .unwrap();
    ws.alarm(ws.next_alarm().unwrap()).unwrap();
    assert_eq!(ws.runs.len(), 1);
    let run = ws.runs[0].view(false).unwrap();
    assert_eq!(run["mode"], "schedule");
    check(&case, &run);
}
#[test]
fn webhook_example_runs_from_an_installation_local_registration() {
    let case = cases()
        .into_iter()
        .find(|c| c["file"] == "workflows/09-webhook.json")
        .unwrap();
    let doc = load(&case);
    let mut ws = Workspace::default();
    save(&mut ws, &doc);
    ws.configure_webhook("09-webhook", "test-endpoint".into(), "test-hash".into())
        .unwrap();
    ws.handle("POST", "/api/workflows/09-webhook/publish", b"", 0, "")
        .unwrap();
    ws.receive_webhook(
        "09-webhook",
        "test-endpoint",
        "example-event",
        json!({}),
        "test-fingerprint",
        0,
        "event-run",
    )
    .unwrap();
    let run = ws.runs[0].view(false).unwrap();
    assert_eq!(run["mode"], "webhook");
    check(&case, &run);
}
