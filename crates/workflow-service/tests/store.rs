//! The per-entity store must reproduce the workspace exactly, and write only what
//! an operation changed.
use serde_json::{Value, json};
use std::rc::Rc;
use std::{fs, path::PathBuf};
use workflow_service::Archive;
use workflow_service::Workspace;
use workflow_service::store::sql::{FORMAT_VERSION, SqlExec, SqlStore, SqlValue, legacy};
use workflow_service::store::{
    ChangeSet, Collection, MemoryStore, RetentionPolicy, RunArchive, RunQueries, RunRetention,
    Store, StoreError, Tracked,
};

/// Every test runs against each store implementation.
trait TestStore: Store + RunQueries<Error = StoreError> + RunArchive {
    fn fresh() -> Self;
    /// A reader of this store's runs, as a host attaches to a loaded workspace.
    fn archive(&self) -> Archive;
}
impl TestStore for MemoryStore {
    fn fresh() -> Self {
        MemoryStore::default()
    }
    fn archive(&self) -> Archive {
        // A copy is current until the next commit, after which `cycle` attaches
        // a new one.
        Archive::new(Rc::new(self.clone()))
    }
}
/// A real SQLite database, standing in for Durable Object SQLite.
/// When `plans` is set, every query's plan is recorded before it runs.
/// When `fail_after` is set, statements fail once that many more have run, to
/// inject a failure part-way through an operation. `uri` names a shared
/// in-memory database, so a second connection (the run archive) sees the same
/// data.
struct Sqlite {
    uri: String,
    connection: rusqlite::Connection,
    plans: std::cell::RefCell<Option<Vec<String>>>,
    fail_after: std::cell::Cell<Option<usize>>,
}
static NEXT_DATABASE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
impl Sqlite {
    fn open(uri: &str) -> Self {
        use rusqlite::OpenFlags;
        Sqlite {
            uri: uri.into(),
            connection: rusqlite::Connection::open_with_flags(
                uri,
                OpenFlags::default()
                    | OpenFlags::SQLITE_OPEN_URI
                    | OpenFlags::SQLITE_OPEN_SHARED_CACHE,
            )
            .unwrap(),
            plans: Default::default(),
            fail_after: Default::default(),
        }
    }
}
impl SqlExec for Sqlite {
    fn query(&self, sql: &str, params: Vec<SqlValue>) -> Result<Vec<Vec<SqlValue>>, StoreError> {
        use rusqlite::types::{Value as V, ValueRef};
        let fail = |e: rusqlite::Error| StoreError::Invalid(format!("{sql}: {e}"));
        if let Some(remaining) = self.fail_after.get() {
            if remaining == 0 {
                return Err(StoreError::Invalid(format!("injected failure at {sql}")));
            }
            self.fail_after.set(Some(remaining - 1));
        }
        let params: Vec<V> = params
            .into_iter()
            .map(|p| match p {
                SqlValue::Null => V::Null,
                SqlValue::Integer(i) => V::Integer(i),
                SqlValue::Real(f) => V::Real(f),
                SqlValue::Text(t) => V::Text(t),
            })
            .collect();
        if let Some(plans) = self.plans.borrow_mut().as_mut() {
            let mut explain = self
                .connection
                .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                .map_err(fail)?;
            let steps: Vec<String> = explain
                .query_map(rusqlite::params_from_iter(params.clone()), |row| row.get(3))
                .map_err(fail)?
                .collect::<Result<_, _>>()
                .map_err(fail)?;
            plans.push(steps.join("; "));
        }
        let mut statement = self.connection.prepare(sql).map_err(fail)?;
        let columns = statement.column_count();
        let mut rows = statement
            .query(rusqlite::params_from_iter(params))
            .map_err(fail)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(fail)? {
            let mut values = Vec::with_capacity(columns);
            for i in 0..columns {
                values.push(match row.get_ref(i).map_err(fail)? {
                    ValueRef::Null => SqlValue::Null,
                    ValueRef::Integer(i) => SqlValue::Integer(i),
                    ValueRef::Real(f) => SqlValue::Real(f),
                    ValueRef::Text(t) => SqlValue::Text(String::from_utf8_lossy(t).into_owned()),
                    ValueRef::Blob(_) => return Err(StoreError::Invalid("unexpected blob".into())),
                });
            }
            out.push(values);
        }
        Ok(out)
    }
}
/// A connection with no tables, for migrations.
fn bare_sqlite() -> SqlStore<Sqlite> {
    SqlStore::new(Sqlite::open(&format!(
        "file:kasuri-{}?mode=memory&cache=shared",
        NEXT_DATABASE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )))
}
fn sqlite() -> SqlStore<Sqlite> {
    let store = bare_sqlite();
    store.create().unwrap();
    store
}
impl TestStore for SqlStore<Sqlite> {
    fn fresh() -> Self {
        sqlite()
    }
    fn archive(&self) -> Archive {
        Archive::new(Rc::new(SqlStore::new(Sqlite::open(&self.sql().uri))))
    }
}

/// Commit, reload from the store, and require the reload to equal the workspace
/// that was committed. Returns the reloaded workspace and what was written.
fn cycle<S: TestStore>(store: &mut S, tracked: &mut Tracked) -> (Tracked, ChangeSet) {
    let written = tracked.commit(store).unwrap();
    let mut reloaded = Tracked::load(store).unwrap();
    reloaded.workspace.archive = store.archive();
    assert_eq!(
        serde_json::to_value(&reloaded.workspace).unwrap(),
        serde_json::to_value(&tracked.workspace).unwrap(),
        "reload differs from the committed workspace"
    );
    assert_queries_match(store, &tracked.workspace);
    (reloaded, written)
}
/// The store answers every run query from its columns exactly as the workspace
/// answers it from runs in memory.
fn assert_queries_match<S: TestStore>(store: &S, ws: &Workspace) {
    assert_eq!(
        store.next_run_wake().unwrap(),
        ws.next_run_wake().unwrap(),
        "next wake"
    );
    let wake = ws.next_run_wake().unwrap();
    for now in [
        Some(0),
        wake,
        wake.map(|w| w.saturating_sub(1)),
        Some(u64::MAX),
    ]
    .into_iter()
    .flatten()
    {
        assert_eq!(
            store.due_runs(now).unwrap(),
            ws.due_runs(now).unwrap(),
            "due at {now}"
        );
    }
    for open in [false, true] {
        assert_eq!(
            store.active_runs(open).unwrap(),
            ws.active_runs(open).unwrap(),
            "active"
        );
    }
    // The store lists archived runs too; the runs in memory appear in it in the
    // same order and with the same entries.
    let resident: Vec<String> = ws.runs.iter().map(|r| r.id_string().unwrap()).collect();
    let stored: Vec<Value> = store
        .run_summaries(usize::MAX)
        .unwrap()
        .into_iter()
        .filter(|v| resident.iter().any(|id| v["id"] == id.as_str()))
        .collect();
    assert_eq!(stored, ws.run_summaries(usize::MAX).unwrap(), "summaries");
    let mut ids: Vec<String> = ws.runs.iter().map(|r| r.id_string().unwrap()).collect();
    ids.push("missing-run".into());
    for id in ids {
        assert_eq!(
            store.run_protected(&id).unwrap(),
            ws.run_protected(&id).unwrap(),
            "protected {id}"
        );
    }
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
/// IDs of the newest `limit` stored runs, archived or not.
fn listed<S: TestStore>(store: &S, limit: usize) -> Vec<String> {
    store
        .list_runs(limit, None, None)
        .unwrap()
        .into_iter()
        .map(|v| v["id"].as_str().unwrap().to_string())
        .collect()
}
fn only(changes: &ChangeSet, collection: Collection) -> bool {
    changes
        .upserts
        .iter()
        .all(|r| r.key.collection == collection)
        && changes.deletes.iter().all(|k| k.collection == collection)
}

fn an_empty_store_is_an_empty_workspace<S: TestStore>() {
    let mut store = S::fresh();
    let mut tracked = Tracked::load(&store).unwrap();
    assert!(tracked.workspace.workflows.is_empty());
    let (_, written) = cycle(&mut store, &mut tracked);
    assert!(written.meta.is_some() && written.upserts.is_empty());
    // Nothing changed, so nothing is written the second time.
    let mut tracked = Tracked::load(&store).unwrap();
    assert!(tracked.commit(&mut store).unwrap().is_empty());
}

fn operations_write_only_the_entities_they_change<S: TestStore>() {
    let mut store = S::fresh();
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

fn newest_first_runs_keep_their_order_and_existing_rows<S: TestStore>() {
    let mut store = S::fresh();
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
    // Finished runs leave memory, and the stored list stays newest first.
    assert!(tracked.workspace.runs.is_empty());
    let expected: Vec<String> = (0..30).rev().map(|i| format!("run-{i:02}")).collect();
    assert_eq!(listed(&store, 100), expected, "newest first, as before");
    // The workspace lists and reads them through its archive.
    let snapshot = handle(
        &mut tracked.workspace,
        "GET",
        "/api/workspace",
        Value::Null,
        30,
        "",
    );
    let ids: Vec<&str> = snapshot["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, expected);
    let run = handle(
        &mut tracked.workspace,
        "GET",
        "/api/runs/run-07",
        Value::Null,
        30,
        "",
    );
    assert_eq!(run["status"], "succeeded");
    let refused = tracked
        .workspace
        .handle("POST", "/api/runs/run-07/cancel", b"", 30, "")
        .unwrap_err();
    assert_eq!(refused.status, 409, "finished runs can't be controlled");
}

fn retention_and_audit_trimming_become_deletes<S: TestStore>() {
    let mut store = S::fresh();
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
    // History keeps the newest 100 runs; finished runs don't stay in memory.
    let (mut tracked, _) = cycle(&mut store, &mut tracked);
    assert!(tracked.workspace.runs.is_empty());
    let kept = listed(&store, 1000);
    assert_eq!(kept.len(), 100);
    assert_eq!(
        (kept[0].as_str(), kept[99].as_str()),
        ("run-104", "run-005")
    );
    handle(
        &mut tracked.workspace,
        "POST",
        "/api/runs",
        json!({"workflowId":"flow","mode":"run","input":{}}),
        200,
        "run-new",
    );
    let (tracked, _) = cycle(&mut store, &mut tracked);
    let kept = listed(&store, 1000);
    assert_eq!(kept.len(), 100);
    assert_eq!(
        (kept[0].as_str(), kept[99].as_str()),
        ("run-new", "run-006"),
        "the oldest run is pruned"
    );

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

fn webhook_rotation_replaces_the_registration_row<S: TestStore>() {
    let mut store = S::fresh();
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

fn reordered_collections_reload_in_the_new_order<S: TestStore>() {
    let mut store = S::fresh();
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

fn duplicate_entity_keys_are_rejected<S: TestStore>() {
    let store = S::fresh();
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
fn step<S: TestStore>(store: &mut S, tracked: Tracked) -> Tracked {
    let mut tracked = tracked;
    cycle(store, &mut tracked).0
}

fn every_published_example_runs_through_the_store<S: TestStore>() {
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
            let mut store = S::fresh();
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

fn webhook_response_windows_are_indexed_until_they_close<S: TestStore>() {
    let graph = json!({"schemaVersion":1,"id":"response","name":"Response","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Webhook","label":"Webhook","config":{"authentication":"none"}},{"id":"wait","type":"Wait / Delay","label":"Wait","config":{"delayMs":6000}},{"id":"respond","type":"Respond to Webhook","label":"Respond","config":{"status":201,"body":{"$input":""}}}],"edges":[{"id":"a","source":"start","target":"wait"},{"id":"b","source":"wait","target":"respond"}]});
    let mut store = S::fresh();
    let mut tracked = Tracked::load(&store).unwrap();
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/response",
        graph,
        0,
        "",
    );
    tracked
        .workspace
        .configure_webhook("response", "endpoint".into(), "hash".into())
        .unwrap();
    handle(
        &mut tracked.workspace,
        "POST",
        "/api/workflows/response/publish",
        Value::Null,
        0,
        "",
    );
    let (mut tracked, _) = cycle(&mut store, &mut tracked);
    tracked
        .workspace
        .receive_webhook_context(
            "response",
            "endpoint",
            json!({"input":1}),
            0,
            "hook-run",
            json!({"trigger":"webhook","request":{}}),
        )
        .unwrap();
    let (mut tracked, _) = cycle(&mut store, &mut tracked);
    // The open window's 5 s deadline comes before the 6 s wait, and the run counts
    // as active only when open responses are counted, or while it runs.
    assert_eq!(store.next_run_wake().unwrap(), Some(5000));
    assert_eq!(
        store.due_runs(0).unwrap(),
        ["hook-run"],
        "open windows are always visited"
    );
    // Past the deadline the window closes and is no longer indexed as open.
    tracked.workspace.alarm(5000).unwrap();
    let (tracked, _) = cycle(&mut store, &mut tracked);
    assert_eq!(store.next_run_wake().unwrap(), Some(6000));
    assert!(store.due_runs(5001).unwrap().is_empty());
    assert_eq!(
        tracked.workspace.runs[0].view(true).unwrap()["webhookResponse"]["delivery"],
        "timed_out"
    );
}

fn awaited_children_and_their_parents_are_indexed_as_protected<S: TestStore>() {
    let child = json!({"schemaVersion":1,"id":"child","name":"Child","status":"draft","updatedAt":"","nodes":[{"id":"trigger","type":"Execute Workflow Trigger","label":"Invoked","config":{"outputNodeId":"result"}},{"id":"wait","type":"Wait / Delay","label":"Wait","config":{"delayMs":100}},{"id":"result","type":"Map / Transform","label":"Result","config":{"mapping":{"answer":42}}}],"edges":[{"id":"a","source":"trigger","target":"wait"},{"id":"b","source":"wait","target":"result"}]});
    let parent = json!({"schemaVersion":1,"id":"parent","name":"Parent","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Manual Trigger","label":"Start","config":{}},{"id":"invoke","type":"Execute Workflow","label":"Call child","config":{"workflowId":"child","mode":"await"}}],"edges":[{"id":"a","source":"start","target":"invoke"}]});
    let mut store = S::fresh();
    let mut tracked = Tracked::load(&store).unwrap();
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/child",
        child,
        0,
        "",
    );
    handle(
        &mut tracked.workspace,
        "POST",
        "/api/workflows/child/publish",
        Value::Null,
        0,
        "",
    );
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/parent",
        parent,
        0,
        "",
    );
    let (mut tracked, _) = cycle(&mut store, &mut tracked);
    let started = handle(
        &mut tracked.workspace,
        "POST",
        "/api/runs",
        json!({"workflowId":"parent","mode":"run","input":{}}),
        0,
        "parent-run",
    );
    let child_id = started["children"][0]["runId"]
        .as_str()
        .unwrap()
        .to_string();
    let (mut tracked, _) = cycle(&mut store, &mut tracked);
    // The running parent awaits the child, and the child's root is the parent.
    assert!(store.run_protected(&child_id).unwrap(), "awaited child");
    assert!(
        store.run_protected("parent-run").unwrap(),
        "invocation root"
    );
    for at in [100, 101] {
        tracked.workspace.alarm(at).unwrap();
        tracked = cycle(&mut store, &mut tracked).0;
    }
    assert_eq!(
        handle(
            &mut tracked.workspace,
            "GET",
            "/api/runs/parent-run",
            Value::Null,
            101,
            ""
        )["status"],
        "succeeded"
    );
    assert!(
        !store.run_protected(&child_id).unwrap(),
        "released once collected"
    );
}

/// Retention during a commit, computed from per-entity sizes, removes exactly what
/// whole-document retention (`Workspace::checkpoint`) removes from what stays in
/// memory. Finished runs leave memory anyway, so audit entries and active runs
/// are what the budget still bounds.
fn byte_retention_matches_whole_document_retention<S: TestStore>() {
    let mut store = S::fresh();
    let mut tracked = Tracked::load(&store).unwrap();
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/flow",
        workflow("flow", 60_000),
        0,
        "",
    );
    let (mut tracked, _) = cycle(&mut store, &mut tracked);
    for i in 0..5 {
        handle(
            &mut tracked.workspace,
            "POST",
            "/api/runs",
            json!({"workflowId":"flow","mode":"run","input":{"pad":"p".repeat(20_000)}}),
            i,
            &format!("run-{i:03}"),
        );
    }
    for i in 0..400 {
        tracked.workspace.record_audit(
            "browser",
            "workflow.saved",
            &"z".repeat(16_000),
            "succeeded",
            100 + i,
            "",
        );
    }
    let mut expected = tracked.workspace.clone();
    let text = expected.checkpoint().unwrap();
    assert!(text.len() <= workflow_service::MAX_STATE_BYTES);
    assert_eq!(expected.runs.len(), 5, "active runs are never removed");
    assert!(
        expected.audit.len() < 400,
        "the budget forces audit entries out"
    );
    let (tracked, written) = cycle(&mut store, &mut tracked);
    assert_eq!(
        serde_json::to_value(&tracked.workspace).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    assert!(
        written
            .deletes
            .iter()
            .all(|k| k.collection == Collection::Audit)
    );
}

macro_rules! for_each_store {
    ($($name:ident: $store:ty),*) => {$(
        mod $name {
            use super::*;
            #[test]
            fn an_empty_store_is_an_empty_workspace() {
                super::super::an_empty_store_is_an_empty_workspace::<$store>();
            }
            #[test]
            fn operations_write_only_the_entities_they_change() {
                super::super::operations_write_only_the_entities_they_change::<$store>();
            }
            #[test]
            fn newest_first_runs_keep_their_order_and_existing_rows() {
                super::super::newest_first_runs_keep_their_order_and_existing_rows::<$store>();
            }
            #[test]
            fn retention_and_audit_trimming_become_deletes() {
                super::super::retention_and_audit_trimming_become_deletes::<$store>();
            }
            #[test]
            fn webhook_rotation_replaces_the_registration_row() {
                super::super::webhook_rotation_replaces_the_registration_row::<$store>();
            }
            #[test]
            fn reordered_collections_reload_in_the_new_order() {
                super::super::reordered_collections_reload_in_the_new_order::<$store>();
            }
            #[test]
            fn duplicate_entity_keys_are_rejected() {
                super::super::duplicate_entity_keys_are_rejected::<$store>();
            }
            #[test]
            fn every_published_example_runs_through_the_store() {
                super::super::every_published_example_runs_through_the_store::<$store>();
            }
            #[test]
            fn webhook_response_windows_are_indexed_until_they_close() {
                super::super::webhook_response_windows_are_indexed_until_they_close::<$store>();
            }
            #[test]
            fn awaited_children_and_their_parents_are_indexed_as_protected() {
                super::super::awaited_children_and_their_parents_are_indexed_as_protected::<$store>();
            }
            #[test]
            fn list_filters_cover_archived_and_active_runs() {
                super::super::list_filters_cover_archived_and_active_runs::<$store>();
            }
            #[test]
            fn pruning_never_removes_runs_in_memory() {
                super::super::pruning_never_removes_runs_in_memory::<$store>();
            }
            #[test]
            fn history_is_kept_per_workflow() {
                super::super::history_is_kept_per_workflow::<$store>();
            }
            #[test]
            fn old_runs_expire_and_failed_runs_are_kept_longer() {
                super::super::old_runs_expire_and_failed_runs_are_kept_longer::<$store>();
            }
            #[test]
            fn byte_retention_matches_whole_document_retention() {
                super::super::byte_retention_matches_whole_document_retention::<$store>();
            }
        }
    )*};
}
mod stores {
    use super::*;
    for_each_store!(memory: MemoryStore, sqlite: SqlStore<Sqlite>);
}

#[test]
fn sqlite_tables_record_their_format_and_commit() {
    let store = sqlite();
    assert_eq!(
        store.format().unwrap(),
        None,
        "created but not yet holding a workspace"
    );
    store.set_format().unwrap();
    store.set_commit("first").unwrap();
    assert_eq!(store.format().unwrap(), Some(FORMAT_VERSION));
    assert_eq!(store.commit().unwrap().as_deref(), Some("first"));
    store.drop_tables().unwrap();
    assert_eq!(store.format().unwrap(), None, "no tables at all");
}

#[test]
fn a_rolled_back_sqlite_commit_leaves_the_previous_workspace() {
    let mut store = sqlite();
    let mut tracked = Tracked::load(&store).unwrap();
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/a",
        workflow("a", 1),
        0,
        "",
    );
    let (before, _) = cycle(&mut store, &mut tracked);

    let mut tracked = before.clone();
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/b",
        workflow("b", 1),
        1,
        "",
    );
    store.sql().query("BEGIN", Vec::new()).unwrap();
    tracked.commit(&mut store).unwrap();
    // The host's transaction fails after the rows were written.
    store.sql().query("ROLLBACK", Vec::new()).unwrap();

    let reloaded = Tracked::load(&store).unwrap();
    assert_eq!(
        serde_json::to_value(&reloaded.workspace).unwrap(),
        serde_json::to_value(&before.workspace).unwrap()
    );
    assert_queries_match(&store, &before.workspace);
}

#[test]
fn sqlite_run_queries_use_indexes() {
    let mut store = sqlite();
    let mut tracked = Tracked::load(&store).unwrap();
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/flow",
        workflow("flow", 50),
        0,
        "",
    );
    for i in 0..20 {
        handle(
            &mut tracked.workspace,
            "POST",
            "/api/runs",
            json!({"workflowId":"flow","mode":"run","input":{}}),
            i,
            &format!("run-{i:02}"),
        );
    }
    cycle(&mut store, &mut tracked);
    *store.sql().plans.borrow_mut() = Some(Vec::new());
    store.next_run_wake().unwrap();
    store.due_runs(10).unwrap();
    store.active_runs(false).unwrap();
    store.active_runs(true).unwrap();
    store.run_summaries(5).unwrap();
    store.run_protected("run-03").unwrap();
    let plans = store.sql().plans.borrow_mut().take().unwrap();
    assert_eq!(plans.len(), 6);
    // Loading the runs held in memory, and listing one workflow's runs, read their
    // own indexes in order.
    *store.sql().plans.borrow_mut() = Some(Vec::new());
    store.records(Collection::Run).unwrap();
    store.list_runs(5, Some("flow"), None).unwrap();
    let ordered = store.sql().plans.borrow_mut().take().unwrap();
    for (plan, index) in ordered.iter().zip(["ws_runs_resident", "ws_runs_workflow"]) {
        assert!(plan.contains(index), "{plan}");
        assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    }
    for (index, plan) in plans.iter().enumerate() {
        let steps: Vec<&str> = plan.split("; ").collect();
        if index == 4 {
            assert!(
                !steps.iter().any(|s| s.starts_with("USE TEMP B-TREE")),
                "run lists come out of the index already ordered: {plan}"
            );
            // Run lists walk the order index and stop at the limit.
            assert!(
                plan.contains("USING COVERING INDEX ws_runs_ord")
                    || plan.contains("USING INDEX ws_runs_ord"),
                "{plan}"
            );
        } else {
            assert!(
                !steps.iter().any(|s| s.starts_with("SCAN ws_runs")),
                "no query visits every run: {plan}"
            );
        }
    }
}

// Workspaces saved by releases before table storage: record generations, and the
// single-row layout before them. This is the frozen SQL those releases wrote with.
const LEGACY_CREATE_RECORDS: &str = "CREATE TABLE IF NOT EXISTS workspace_records (generation INTEGER NOT NULL, kind TEXT NOT NULL, position INTEGER NOT NULL, body TEXT NOT NULL, PRIMARY KEY(generation,kind,position))";
const LEGACY_STORE_RECORDS: &str = r#"
WITH input(body,generation) AS (
  SELECT json(?), COALESCE((SELECT MAX(generation) FROM workspace_records),0)+1
), records(kind,position,body) AS (
  SELECT 'metadata',0,json_set(json_remove(body,'$.workflows','$.runs','$.audit','$.webhooks','$.webhookEvents','$.revisions'),'$.storageVersion',1) FROM input
  UNION ALL
  SELECT CASE collection.key
    WHEN 'workflows' THEN 'workflow' WHEN 'runs' THEN 'run'
    WHEN 'audit' THEN 'audit' WHEN 'webhooks' THEN 'webhook'
    WHEN 'webhookEvents' THEN 'webhook_event' WHEN 'revisions' THEN 'revision' END,
    CAST(item.key AS INTEGER),item.value
  FROM input,json_each(input.body) AS collection,
    json_each(CASE WHEN collection.key IN ('workflows','runs','audit','webhooks','webhookEvents','revisions') THEN collection.value ELSE '[]' END) AS item
)
INSERT INTO workspace_records(generation,kind,position,body)
SELECT input.generation,records.kind,records.position,records.body FROM input,records
"#;

fn exec(store: &SqlStore<Sqlite>, sql: &str, params: Vec<SqlValue>) {
    store.sql().query(sql, params).unwrap();
}
fn table_exists(store: &SqlStore<Sqlite>, name: &str) -> bool {
    !store
        .sql()
        .query(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?",
            vec![SqlValue::Text(name.into())],
        )
        .unwrap()
        .is_empty()
}
/// A workspace with a published workflow, a waiting run, an audit entry and a
/// webhook, as the legacy layouts stored it.
fn legacy_workspace() -> Workspace {
    let mut ws = Workspace::default();
    handle(
        &mut ws,
        "PUT",
        "/api/workflows/wait",
        workflow("wait", 5_000),
        0,
        "",
    );
    handle(
        &mut ws,
        "POST",
        "/api/workflows/wait/publish",
        Value::Null,
        0,
        "",
    );
    let mut hook = workflow("hook", 0);
    hook["nodes"][0] =
        json!({"id":"start","type":"Webhook","label":"Start","config":{"authentication":"bearer"}});
    handle(&mut ws, "PUT", "/api/workflows/hook", hook, 0, "");
    ws.configure_webhook("hook", "endpoint".into(), "hash".into())
        .unwrap();
    handle(
        &mut ws,
        "POST",
        "/api/runs",
        json!({"workflowId":"wait","mode":"run","input":{"legacy":true}}),
        1,
        "legacy-run",
    );
    ws.checkpoint().unwrap();
    ws
}
fn store_generation(store: &SqlStore<Sqlite>, ws: &mut Workspace) {
    exec(store, LEGACY_CREATE_RECORDS, Vec::new());
    exec(
        store,
        LEGACY_STORE_RECORDS,
        vec![SqlValue::Text(ws.checkpoint().unwrap())],
    );
}
fn same(a: &Workspace, b: &Workspace) {
    assert_eq!(
        serde_json::to_value(a).unwrap(),
        serde_json::to_value(b).unwrap()
    );
}

#[test]
fn record_generations_migrate_into_tables() {
    let mut store = bare_sqlite();
    let mut old = legacy_workspace();
    let mut stale = old.clone();
    stale.workflows.clear();
    store_generation(&store, &mut stale);
    // The latest generation wins; the older one must not leak into the result.
    store_generation(&store, &mut old);
    let tracked = legacy::migrate(&mut store).unwrap();
    same(&tracked.workspace, &old);
    assert_eq!(store.format().unwrap(), Some(FORMAT_VERSION));
    assert!(!table_exists(&store, "workspace_records"));
    same(&Tracked::load(&store).unwrap().workspace, &old);
    assert_queries_match(&store, &old);
    assert_eq!(
        store.due_runs(5_001).unwrap(),
        ["legacy-run"],
        "the wait survives"
    );
}

#[test]
fn the_single_row_layout_migrates_into_tables() {
    let mut store = bare_sqlite();
    let mut old = legacy_workspace();
    exec(
        &store,
        "CREATE TABLE workspace (id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL)",
        Vec::new(),
    );
    exec(
        &store,
        "INSERT INTO workspace VALUES(1,?)",
        vec![SqlValue::Text(old.checkpoint().unwrap())],
    );
    let tracked = legacy::migrate(&mut store).unwrap();
    same(&tracked.workspace, &old);
    assert!(!table_exists(&store, "workspace"));
    assert_queries_match(&store, &old);
}

#[test]
fn a_new_database_migrates_to_an_empty_workspace() {
    let mut store = bare_sqlite();
    let tracked = legacy::migrate(&mut store).unwrap();
    assert!(tracked.workspace.workflows.is_empty() && tracked.workspace.runs.is_empty());
    assert_eq!(store.format().unwrap(), Some(FORMAT_VERSION));
}

#[test]
fn corrupt_generations_are_not_migrated_as_empty_workspaces() {
    for rows in [
        vec![],
        vec![("metadata", json!({"storageVersion":2}))],
        vec![
            ("metadata", json!({"storageVersion":1})),
            ("unknown", json!({})),
        ],
        vec![
            ("metadata", json!({"storageVersion":1})),
            ("metadata", json!({"storageVersion":1})),
        ],
    ] {
        let rows = rows
            .into_iter()
            .map(|(kind, body)| (kind.to_string(), body.to_string()))
            .collect();
        assert!(legacy::decode(rows).is_err());
    }
    let ws = legacy::decode(vec![
        (
            "audit".into(),
            json!({"id":7,"action":"workflow.saved"}).to_string(),
        ),
        (
            "metadata".into(),
            json!({"storageVersion":1,"management":{"private":"stored"},"auditSequence":7})
                .to_string(),
        ),
    ])
    .unwrap();
    assert_eq!((ws.audit.len(), ws.audit_sequence), (1, 7));
    assert_eq!(ws.management["private"], "stored");
}

#[test]
fn a_failure_part_way_through_migration_leaves_the_legacy_layout() {
    let mut old = legacy_workspace();
    let setup = bare_sqlite();
    store_generation(&setup, &mut old);
    // Count the statements a successful migration runs, then fail at each one.
    let statements = {
        let mut probe = bare_sqlite();
        store_generation(&probe, &mut old.clone());
        probe.sql().fail_after.set(Some(usize::MAX));
        legacy::migrate(&mut probe).unwrap();
        usize::MAX - probe.sql().fail_after.get().unwrap()
    };
    assert!(statements > 10, "{statements}");
    for at in 0..statements {
        let mut store = bare_sqlite();
        store_generation(&store, &mut old.clone());
        exec(&store, "BEGIN", Vec::new());
        store.sql().fail_after.set(Some(at));
        assert!(legacy::migrate(&mut store).is_err(), "statement {at}");
        store.sql().fail_after.set(None);
        // The host's transaction fails, so it rolls back.
        exec(&store, "ROLLBACK", Vec::new());
        assert_eq!(store.format().unwrap(), None, "statement {at}");
        same(&legacy::read(store.sql()).unwrap().unwrap(), &old);
    }
}

/// The table layout (format 1) written before runs could be archived.
const FORMAT_1_SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS ws_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS ws_workflows (id TEXT PRIMARY KEY, ord INTEGER NOT NULL, body TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS ws_revisions (id TEXT PRIMARY KEY, ord INTEGER NOT NULL, body TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS ws_audit (id TEXT PRIMARY KEY, ord INTEGER NOT NULL, body TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS ws_webhooks (id TEXT PRIMARY KEY, ord INTEGER NOT NULL, body TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS ws_runs (id TEXT PRIMARY KEY, ord INTEGER NOT NULL, status TEXT NOT NULL, created_at INTEGER NOT NULL, wake_at INTEGER, response_open INTEGER NOT NULL, response_deadline INTEGER, parent_id TEXT, root_id TEXT, summary TEXT NOT NULL, body TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS ws_run_awaits (child_id TEXT NOT NULL, run_id TEXT NOT NULL, PRIMARY KEY(child_id, run_id))",
    "CREATE INDEX IF NOT EXISTS ws_runs_ord ON ws_runs(ord, id)",
    "CREATE INDEX IF NOT EXISTS ws_runs_status ON ws_runs(status)",
    "CREATE INDEX IF NOT EXISTS ws_runs_wake_at ON ws_runs(wake_at)",
    "CREATE INDEX IF NOT EXISTS ws_runs_response ON ws_runs(response_open, response_deadline)",
    "CREATE INDEX IF NOT EXISTS ws_runs_parent ON ws_runs(parent_id)",
    "CREATE INDEX IF NOT EXISTS ws_runs_root ON ws_runs(root_id)",
    "CREATE INDEX IF NOT EXISTS ws_run_awaits_run ON ws_run_awaits(run_id)",
];

#[test]
fn format_1_tables_upgrade_in_place() {
    // Write a workspace with a finished and an active run as format 1 stored it:
    // every run in memory, no workflow or archived columns.
    let mut ws = Workspace::default();
    handle(
        &mut ws,
        "PUT",
        "/api/workflows/wait",
        workflow("wait", 5_000),
        0,
        "",
    );
    handle(
        &mut ws,
        "PUT",
        "/api/workflows/flow",
        instant("flow"),
        0,
        "",
    );
    handle(
        &mut ws,
        "POST",
        "/api/runs",
        json!({"workflowId":"flow","mode":"run","input":{}}),
        1,
        "done",
    );
    handle(
        &mut ws,
        "POST",
        "/api/runs",
        json!({"workflowId":"wait","mode":"run","input":{}}),
        2,
        "waiting",
    );
    let mut reference = sqlite();
    let mut tracked = Tracked::load(&reference).unwrap();
    tracked.workspace = ws.clone();
    tracked.commit(&mut reference).unwrap();

    let mut store = bare_sqlite();
    for statement in FORMAT_1_SCHEMA {
        exec(&store, statement, Vec::new());
    }
    for (key, value) in [
        ("format", "1"),
        ("workspace", &*Store::meta(&reference).unwrap().unwrap()),
    ] {
        exec(
            &store,
            "INSERT INTO ws_meta VALUES(?,?)",
            vec![SqlValue::Text(key.into()), SqlValue::Text(value.into())],
        );
    }
    for (order, record) in ws.runs.iter().enumerate() {
        let summary = record.view(true).unwrap().to_string();
        exec(
            &store,
            "INSERT INTO ws_runs(id,ord,status,created_at,wake_at,response_open,response_deadline,parent_id,root_id,summary,body) VALUES(?,?,?,?,?,0,NULL,NULL,NULL,?,?)",
            vec![
                SqlValue::Text(record.id_string().unwrap()),
                SqlValue::Integer(order as i64),
                SqlValue::Text(
                    serde_json::to_value(record.status().unwrap())
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .into(),
                ),
                SqlValue::Integer(record.created_at as i64),
                record
                    .wake_at
                    .map_or(SqlValue::Null, |w| SqlValue::Integer(w as i64)),
                SqlValue::Text(summary),
                SqlValue::Text(serde_json::to_string(record).unwrap()),
            ],
        );
    }
    for workflow in &ws.workflows {
        exec(
            &store,
            "INSERT INTO ws_workflows VALUES(?,?,?)",
            vec![
                SqlValue::Text(workflow.graph.id.clone()),
                SqlValue::Integer(0),
                SqlValue::Text(serde_json::to_string(workflow).unwrap()),
            ],
        );
    }
    assert_eq!(store.format().unwrap(), Some(1));
    assert!(!store.current().unwrap());

    assert!(store.upgrade().unwrap());
    assert!(store.current().unwrap());
    assert!(!store.upgrade().unwrap(), "upgrading twice changes nothing");
    // Every run is still loaded until the next commit archives the finished one.
    let mut tracked = Tracked::load(&store).unwrap();
    assert_eq!(tracked.workspace.runs.len(), 2);
    assert_eq!(
        store.list_runs(10, Some("wait"), None).unwrap().len(),
        1,
        "workflow IDs backfilled"
    );
    tracked.commit(&mut store).unwrap();
    let reloaded = Tracked::load(&store).unwrap();
    let ids: Vec<String> = reloaded
        .workspace
        .runs
        .iter()
        .map(|r| r.id_string().unwrap())
        .collect();
    assert_eq!(ids, ["waiting"]);
    assert_eq!(
        store.stored_run("done").unwrap().unwrap().status().unwrap(),
        workflow_engine::RunStatus::Succeeded
    );
}

fn list_filters_cover_archived_and_active_runs<S: TestStore>() {
    let mut store = S::fresh();
    let mut tracked = Tracked::load(&store).unwrap();
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/wait",
        workflow("wait", 5_000),
        0,
        "",
    );
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/flow",
        instant("flow"),
        0,
        "",
    );
    for (i, id) in ["wait", "flow", "wait", "flow", "flow"]
        .into_iter()
        .enumerate()
    {
        handle(
            &mut tracked.workspace,
            "POST",
            "/api/runs",
            json!({"workflowId":id,"mode":"run","input":{}}),
            i as u64,
            &format!("{id}-{i}"),
        );
    }
    let (tracked, _) = cycle(&mut store, &mut tracked);
    assert_eq!(
        tracked.workspace.runs.len(),
        2,
        "the waiting runs stay in memory"
    );
    let ids = |workflow: Option<&str>, status: Option<&str>| -> Vec<String> {
        tracked
            .workspace
            .list_runs(10, workflow, status)
            .unwrap()
            .into_iter()
            .map(|v| v["id"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        ids(None, None),
        ["flow-4", "flow-3", "wait-2", "flow-1", "wait-0"]
    );
    assert_eq!(ids(Some("flow"), None), ["flow-4", "flow-3", "flow-1"]);
    assert_eq!(ids(None, Some("running")), ["wait-2", "wait-0"]);
    assert_eq!(ids(Some("flow"), Some("running")), Vec::<String>::new());
    assert_eq!(tracked.workspace.list_runs(2, None, None).unwrap().len(), 2);
}

fn pruning_never_removes_runs_in_memory<S: TestStore>() {
    let mut store = S::fresh();
    let mut tracked = Tracked::load(&store).unwrap();
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/wait",
        workflow("wait", 50_000),
        0,
        "",
    );
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/flow",
        instant("flow"),
        0,
        "",
    );
    for i in 0..3 {
        handle(
            &mut tracked.workspace,
            "POST",
            "/api/runs",
            json!({"workflowId":"wait","mode":"run","input":{}}),
            i,
            &format!("old-{i}"),
        );
    }
    for i in 0..120 {
        handle(
            &mut tracked.workspace,
            "POST",
            "/api/runs",
            json!({"workflowId":"flow","mode":"run","input":{}}),
            10 + i,
            &format!("new-{i:03}"),
        );
    }
    let (tracked, _) = cycle(&mut store, &mut tracked);
    let kept = listed(&store, 1000);
    // The newest 100 runs, plus the three older runs that are still waiting.
    assert_eq!(kept.len(), 103);
    assert_eq!(&kept[100..], ["old-2", "old-1", "old-0"]);
    assert_eq!(tracked.workspace.runs.len(), 3);
}

#[test]
fn retention_settings_parse_strictly() {
    let day = 24 * 60 * 60 * 1000;
    assert_eq!(
        RetentionPolicy::parse("", "", "").unwrap(),
        RetentionPolicy::default()
    );
    assert_eq!(RetentionPolicy::default().per_workflow, Some(100));
    let policy = RetentionPolicy::parse("25", "7", "").unwrap();
    assert_eq!(
        (
            policy.per_workflow,
            policy.max_age_ms,
            policy.failed_max_age_ms
        ),
        (Some(25), Some(7 * day), Some(7 * day))
    );
    let policy = RetentionPolicy::parse("unlimited", "0", "30").unwrap();
    assert_eq!(
        (
            policy.per_workflow,
            policy.max_age_ms,
            policy.failed_max_age_ms
        ),
        (None, None, Some(30 * day))
    );
    for (history, days, failed) in [
        ("-1", "", ""),
        ("ten", "", ""),
        ("", "1.5", ""),
        ("", "", "soon"),
    ] {
        assert!(
            RetentionPolicy::parse(history, days, failed).is_err(),
            "{history} {days} {failed}"
        );
    }
    let cutoffs = RetentionPolicy::parse("", "1", "3").unwrap().at(10 * day);
    assert_eq!(
        (cutoffs.created_before, cutoffs.failed_created_before),
        (Some(9 * day), Some(7 * day))
    );
    assert!(cutoffs.expired("succeeded", 9 * day - 1) && !cutoffs.expired("succeeded", 9 * day));
    assert!(!cutoffs.expired("failed", 9 * day - 1) && cutoffs.expired("failed", 7 * day - 1));
}

/// A workflow whose runs fail immediately.
fn failing(id: &str) -> Value {
    let mut doc = instant(id);
    doc["nodes"].as_array_mut().unwrap().push(json!({"id":"parse","type":"JSON Utilities","label":"Parse","config":{"operation":"parse"}}));
    doc["edges"] = json!([{"id":"e","source":"start","target":"parse"}]);
    doc
}

fn history_is_kept_per_workflow<S: TestStore>() {
    let mut store = S::fresh();
    let mut tracked = Tracked::load(&store).unwrap();
    tracked.workspace.retention = RunRetention {
        per_workflow: Some(3),
        ..RunRetention::default()
    };
    for id in ["a", "b"] {
        handle(
            &mut tracked.workspace,
            "PUT",
            &format!("/api/workflows/{id}"),
            instant(id),
            0,
            "",
        );
    }
    let (mut tracked, _) = cycle(&mut store, &mut tracked);
    tracked.workspace.retention = RunRetention {
        per_workflow: Some(3),
        ..RunRetention::default()
    };
    for i in 0..5 {
        for id in ["a", "b"] {
            handle(
                &mut tracked.workspace,
                "POST",
                "/api/runs",
                json!({"workflowId":id,"mode":"run","input":{}}),
                i,
                &format!("{id}-{i}"),
            );
        }
    }
    cycle(&mut store, &mut tracked);
    assert_eq!(
        listed(&store, 100),
        ["b-4", "a-4", "b-3", "a-3", "b-2", "a-2"],
        "three of each workflow"
    );
}

fn old_runs_expire_and_failed_runs_are_kept_longer<S: TestStore>() {
    let mut store = S::fresh();
    let mut tracked = Tracked::load(&store).unwrap();
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/ok",
        instant("ok"),
        0,
        "",
    );
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/bad",
        failing("bad"),
        0,
        "",
    );
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/wait",
        workflow("wait", 1_000_000),
        0,
        "",
    );
    for (at, id) in [
        (100, "ok"),
        (100, "bad"),
        (100, "wait"),
        (200, "ok"),
        (200, "bad"),
        (300, "ok"),
        (300, "bad"),
    ] {
        handle(
            &mut tracked.workspace,
            "POST",
            "/api/runs",
            json!({"workflowId":id,"mode":"run","input":{}}),
            at,
            &format!("{id}-{at}"),
        );
    }
    let (mut tracked, _) = cycle(&mut store, &mut tracked);
    assert_eq!(
        store
            .stored_run("bad-100")
            .unwrap()
            .unwrap()
            .status()
            .unwrap(),
        workflow_engine::RunStatus::Failed
    );
    assert_eq!(listed(&store, 100).len(), 7);
    // Finished runs created before 250 expire; failed ones only before 150. The
    // waiting run is in memory and is never deleted, however old.
    tracked.workspace.retention = RunRetention {
        per_workflow: None,
        created_before: Some(250),
        failed_created_before: Some(150),
    };
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/other",
        instant("other"),
        400,
        "",
    );
    cycle(&mut store, &mut tracked);
    assert_eq!(
        listed(&store, 100),
        ["bad-300", "ok-300", "bad-200", "wait-100"]
    );
}

#[test]
fn format_2_and_3_tables_upgrade_in_place() {
    for format in ["2", "3"] {
        let store = sqlite();
        // Tables as those formats wrote them: no remote-run columns, and before
        // format 3 no age index.
        for statement in [
            "DROP INDEX ws_runs_remote",
            "ALTER TABLE ws_runs DROP COLUMN remote",
            "ALTER TABLE ws_runs DROP COLUMN report_version",
        ] {
            exec(&store, statement, Vec::new());
        }
        if format == "2" {
            exec(&store, "DROP INDEX ws_runs_age", Vec::new());
        }
        exec(
            &store,
            &format!("INSERT OR REPLACE INTO ws_meta VALUES('format','{format}')"),
            Vec::new(),
        );
        assert!(!store.current().unwrap());
        assert!(store.upgrade().unwrap());
        assert!(store.current().unwrap());
        assert!(!store.upgrade().unwrap(), "upgrading twice changes nothing");
        for index in ["ws_runs_age", "ws_runs_remote"] {
            assert!(
                !store
                    .sql()
                    .query(
                        "SELECT 1 FROM sqlite_master WHERE type='index' AND name=?",
                        vec![SqlValue::Text(index.into())]
                    )
                    .unwrap()
                    .is_empty(),
                "{index} after upgrading format {format}"
            );
        }
        store
            .sql()
            .query("SELECT remote, report_version FROM ws_runs", Vec::new())
            .unwrap();
        exec(
            &store,
            "INSERT OR REPLACE INTO ws_meta VALUES('format','99')",
            Vec::new(),
        );
        assert!(
            store.upgrade().is_err(),
            "a newer format is refused, not downgraded"
        );
    }
}

#[test]
fn age_pruning_uses_its_index() {
    let mut store = sqlite();
    *store.sql().plans.borrow_mut() = Some(Vec::new());
    store
        .prune_runs(
            &RunRetention {
                per_workflow: Some(5),
                created_before: Some(10),
                failed_created_before: Some(5),
            },
            &["flow".into()],
        )
        .unwrap();
    let plans = store.sql().plans.borrow_mut().take().unwrap();
    assert_eq!(plans.len(), 3);
    assert!(plans[0].contains("ws_runs_workflow"), "{}", plans[0]);
    for plan in &plans[1..] {
        assert!(
            plan.contains("ws_runs_age") && !plan.contains("SCAN ws_runs"),
            "{plan}"
        );
    }
}
