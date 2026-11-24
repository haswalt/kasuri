//! The per-entity store must reproduce the workspace exactly, and write only what
//! an operation changed.
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use workflow_service::Workspace;
use workflow_service::store::{ChangeSet, Collection, MemoryStore, StoreError, Tracked};

/// Commit, reload from the store, and require the reload to equal the workspace
/// that was committed. Returns the reloaded workspace and what was written.
fn cycle(store: &mut MemoryStore, tracked: &mut Tracked) -> (Tracked, ChangeSet) {
    let written = tracked.commit(store).unwrap();
    let reloaded = Tracked::load(store).unwrap();
    assert_eq!(
        serde_json::to_value(&reloaded.workspace).unwrap(),
        serde_json::to_value(&tracked.workspace).unwrap(),
        "reload differs from the committed workspace"
    );
    (reloaded, written)
}
fn handle(
    ws: &mut Workspace,
    method: &str,
    path: &str,
    body: Value,
    now: u64,
    nonce: &str,
) -> Value {
    let body = if body.is_null() {
        Vec::new()
    } else {
        serde_json::to_vec(&body).unwrap()
    };
    ws.handle(method, path, &body, now, nonce).unwrap().1
}
fn workflow(id: &str, delay: u64) -> Value {
    json!({"schemaVersion":1,"id":id,"name":id,"status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Manual Trigger","label":"Start","config":{}},{"id":"wait","type":"Wait / Delay","label":"Wait","config":{"delayMs":delay}}],"edges":[{"id":"e","source":"start","target":"wait"}]})
}
/// A workflow whose runs finish immediately, so any number can be started.
fn instant(id: &str) -> Value {
    let mut doc = workflow(id, 0);
    doc["nodes"].as_array_mut().unwrap().pop();
    doc["edges"] = json!([]);
    doc
}
fn only(changes: &ChangeSet, collection: Collection) -> bool {
    changes
        .upserts
        .iter()
        .all(|r| r.key.collection == collection)
        && changes.deletes.iter().all(|k| k.collection == collection)
}

#[test]
fn an_empty_store_is_an_empty_workspace() {
    let mut store = MemoryStore::default();
    let mut tracked = Tracked::load(&store).unwrap();
    assert!(tracked.workspace.workflows.is_empty());
    let (_, written) = cycle(&mut store, &mut tracked);
    assert!(written.meta.is_some() && written.upserts.is_empty());
    // Nothing changed, so nothing is written the second time.
    let mut tracked = Tracked::load(&store).unwrap();
    assert!(tracked.commit(&mut store).unwrap().is_empty());
}

#[test]
fn operations_write_only_the_entities_they_change() {
    let mut store = MemoryStore::default();
    let mut tracked = Tracked::load(&store).unwrap();
    for id in ["alpha", "beta", "gamma"] {
        handle(
            &mut tracked.workspace,
            "PUT",
            &format!("/api/workflows/{id}"),
            workflow(id, 100),
            0,
            "",
        );
    }
    let (mut tracked, _) = cycle(&mut store, &mut tracked);

    // Starting a run inserts exactly one run row; workflows are not rewritten.
    handle(
        &mut tracked.workspace,
        "POST",
        "/api/runs",
        json!({"workflowId":"alpha","mode":"run","input":{}}),
        1,
        "run-1",
    );
    let (mut tracked, written) = cycle(&mut store, &mut tracked);
    let runs: Vec<_> = written
        .upserts
        .iter()
        .filter(|r| r.key.collection == Collection::Run)
        .collect();
    assert_eq!(runs.len(), 1);
    assert!(
        !written
            .upserts
            .iter()
            .any(|r| r.key.collection == Collection::Workflow)
    );

    // Advancing that run on its alarm rewrites only that run.
    let at = tracked.workspace.next_alarm().unwrap();
    tracked.workspace.alarm(at).unwrap();
    let (mut tracked, written) = cycle(&mut store, &mut tracked);
    assert!(only(&written, Collection::Run), "{written:?}");
    assert_eq!(written.upserts.len(), 1);
    assert_eq!(written.upserts[0].key.id, "run-1");

    // Editing one workflow rewrites that workflow (plus its audit entry), not the others.
    let mut changed = workflow("beta", 200);
    changed["name"] = json!("Beta renamed");
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/beta",
        changed,
        2,
        "",
    );
    let (_, written) = cycle(&mut store, &mut tracked);
    let workflows: Vec<_> = written
        .upserts
        .iter()
        .filter(|r| r.key.collection == Collection::Workflow)
        .map(|r| r.key.id.as_str())
        .collect();
    assert_eq!(workflows, ["beta"]);
    assert!(
        !written
            .upserts
            .iter()
            .any(|r| r.key.collection == Collection::Run)
    );
}

#[test]
fn newest_first_runs_keep_their_order_and_existing_rows() {
    let mut store = MemoryStore::default();
    let mut tracked = Tracked::load(&store).unwrap();
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/flow",
        instant("flow"),
        0,
        "",
    );
    let (mut tracked, _) = cycle(&mut store, &mut tracked);
    for i in 0..30 {
        handle(
            &mut tracked.workspace,
            "POST",
            "/api/runs",
            json!({"workflowId":"flow","mode":"run","input":{}}),
            i,
            &format!("run-{i:02}"),
        );
        let (next, written) = cycle(&mut store, &mut tracked);
        // Prepending a run never renumbers the runs already stored.
        assert_eq!(
            written
                .upserts
                .iter()
                .filter(|r| r.key.collection == Collection::Run)
                .count(),
            1,
            "run {i}"
        );
        tracked = next;
    }
    let ids: Vec<String> = tracked
        .workspace
        .runs
        .iter()
        .map(|r| r.id_string().unwrap())
        .collect();
    let expected: Vec<String> = (0..30).rev().map(|i| format!("run-{i:02}")).collect();
    assert_eq!(ids, expected, "newest first, as before");
}

#[test]
fn retention_and_audit_trimming_become_deletes() {
    let mut store = MemoryStore::default();
    let mut tracked = Tracked::load(&store).unwrap();
    // Terminal runs beyond the 100-run retention are pruned on commit.
    let doc = instant("flow");
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/flow",
        doc,
        0,
        "",
    );
    for i in 0..105 {
        handle(
            &mut tracked.workspace,
            "POST",
            "/api/runs",
            json!({"workflowId":"flow","mode":"run","input":{}}),
            i,
            &format!("run-{i:03}"),
        );
    }
    let (mut tracked, _) = cycle(&mut store, &mut tracked);
    assert_eq!(tracked.workspace.runs.len(), 100);
    handle(
        &mut tracked.workspace,
        "POST",
        "/api/runs",
        json!({"workflowId":"flow","mode":"run","input":{}}),
        200,
        "run-new",
    );
    let (tracked, written) = cycle(&mut store, &mut tracked);
    assert_eq!(tracked.workspace.runs.len(), 100);
    let deleted: Vec<_> = written.deletes.iter().map(|k| k.id.as_str()).collect();
    assert_eq!(deleted, ["run-005"], "the oldest retained run is deleted");

    // Audit retention (1,000 entries) removes the oldest rows.
    let mut tracked = tracked;
    for i in 0..1001 {
        tracked.workspace.record_audit(
            "browser",
            "workflow.saved",
            "flow",
            "succeeded",
            300 + i,
            "",
        );
    }
    let (tracked, written) = cycle(&mut store, &mut tracked);
    assert_eq!(tracked.workspace.audit.len(), 1000);
    assert!(
        written
            .deletes
            .iter()
            .all(|k| k.collection == Collection::Audit)
    );
    assert!(!written.deletes.is_empty());
}

#[test]
fn webhook_rotation_replaces_the_registration_row() {
    let mut store = MemoryStore::default();
    let mut tracked = Tracked::load(&store).unwrap();
    let mut doc = workflow("hook", 0);
    doc["nodes"][0] =
        json!({"id":"start","type":"Webhook","label":"Start","config":{"authentication":"bearer"}});
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/hook",
        doc,
        0,
        "",
    );
    tracked
        .workspace
        .configure_webhook("hook", "endpoint-1".into(), "hash-1".into())
        .unwrap();
    let (mut tracked, _) = cycle(&mut store, &mut tracked);
    tracked
        .workspace
        .configure_webhook("hook", "endpoint-2".into(), "hash-2".into())
        .unwrap();
    let (tracked, written) = cycle(&mut store, &mut tracked);
    assert_eq!(
        written
            .deletes
            .iter()
            .map(|k| k.id.as_str())
            .collect::<Vec<_>>(),
        ["endpoint-1"]
    );
    assert!(written.upserts.iter().any(|r| r.key.id == "endpoint-2"));
    assert_eq!(tracked.workspace.webhooks.len(), 1);
}

#[test]
fn reordered_collections_reload_in_the_new_order() {
    let mut store = MemoryStore::default();
    let mut tracked = Tracked::load(&store).unwrap();
    for id in ["a", "b", "c"] {
        handle(
            &mut tracked.workspace,
            "PUT",
            &format!("/api/workflows/{id}"),
            workflow(id, 1),
            0,
            "",
        );
    }
    let (mut tracked, _) = cycle(&mut store, &mut tracked);
    tracked.workspace.workflows.swap(0, 2);
    let (tracked, _) = cycle(&mut store, &mut tracked);
    let ids: Vec<_> = tracked
        .workspace
        .workflows
        .iter()
        .map(|w| w.graph.id.as_str())
        .collect();
    assert_eq!(ids, ["c", "b", "a"]);
}

#[test]
fn duplicate_entity_keys_are_rejected() {
    let store = MemoryStore::default();
    let mut tracked = Tracked::load(&store).unwrap();
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/a",
        workflow("a", 1),
        0,
        "",
    );
    let copy = tracked.workspace.workflows[0].clone();
    tracked.workspace.workflows.push(copy);
    assert!(matches!(
        tracked.changes(),
        Err(StoreError::DuplicateKey(_))
    ));
}

// Every public example, with a store commit and reload after every step.
fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}
fn read(path: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(examples().join(path)).unwrap()).unwrap()
}
fn step(store: &mut MemoryStore, tracked: Tracked) -> Tracked {
    let mut tracked = tracked;
    cycle(store, &mut tracked).0
}

#[test]
fn every_published_example_runs_through_the_store() {
    let cases: Vec<Value> = read("scenarios.json").as_array().unwrap().clone();
    assert!(!cases.is_empty());
    for case in cases {
        let doc = read(case["file"].as_str().unwrap());
        let id = doc["id"].as_str().unwrap().to_string();
        let has_http = doc["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["type"] == "HTTP Request");
        for mode in ["simulation", "run"] {
            if mode == "run" && has_http {
                continue; // real HTTP is not available in native tests
            }
            let mut store = MemoryStore::default();
            let loaded = Tracked::load(&store).unwrap();
            let mut tracked = step(&mut store, loaded);
            for file in case["dependencies"].as_array().into_iter().flatten() {
                let dep = read(file.as_str().unwrap());
                let dep_id = dep["id"].as_str().unwrap();
                handle(
                    &mut tracked.workspace,
                    "PUT",
                    &format!("/api/workflows/{dep_id}"),
                    dep.clone(),
                    0,
                    "",
                );
                tracked = step(&mut store, tracked);
                handle(
                    &mut tracked.workspace,
                    "POST",
                    &format!("/api/workflows/{dep_id}/publish"),
                    Value::Null,
                    0,
                    "",
                );
                tracked = step(&mut store, tracked);
            }
            handle(
                &mut tracked.workspace,
                "PUT",
                &format!("/api/workflows/{id}"),
                doc.clone(),
                0,
                "",
            );
            tracked = step(&mut store, tracked);
            if mode == "simulation" {
                handle(
                    &mut tracked.workspace,
                    "POST",
                    &format!("/api/workflows/{id}/publish"),
                    Value::Null,
                    0,
                    "",
                );
                tracked = step(&mut store, tracked);
            }
            handle(
                &mut tracked.workspace,
                "POST",
                "/api/runs",
                json!({"workflowId":id,"mode":mode,"input":{}}),
                0,
                "example-run",
            );
            tracked = step(&mut store, tracked);
            let mut data = workflow_data::MemoryStore::default();
            let mut time = 0;
            for _ in 0..40 {
                while let Some((run, task, _, at)) = tracked.workspace.pending_data(time).unwrap() {
                    let prepared = connectors_data::prepare(&task.node, &task.input).unwrap();
                    let result =
                        connectors_data::apply(&mut data, prepared, &task.idempotency_key, at)
                            .unwrap();
                    tracked
                        .workspace
                        .complete_data(&run, &task.id, connector_sdk::Outcome::success(result), at)
                        .unwrap();
                    tracked = step(&mut store, tracked);
                }
                let run = handle(
                    &mut tracked.workspace,
                    "GET",
                    "/api/runs/example-run",
                    Value::Null,
                    time,
                    "",
                );
                if run["status"] != "running" {
                    break;
                }
                let Some(at) = tracked.workspace.next_alarm() else {
                    break;
                };
                time = at;
                tracked.workspace.alarm(at).unwrap();
                tracked = step(&mut store, tracked);
            }
            let run = handle(
                &mut tracked.workspace,
                "GET",
                "/api/runs/example-run",
                Value::Null,
                time,
                "",
            );
            assert_eq!(
                run["status"], "succeeded",
                "{} ({mode}): {run}",
                case["file"]
            );
        }
    }
}
