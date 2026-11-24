//! Runs executed outside the workspace: the workspace keeps only an index row per
//! run, kept current by the run store's reports.
use connector_sdk::{Outcome, Task};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use workflow_service::execution::{
    self, Progress, RunStore, SaveError, Step, WorkBus, WorkMessage,
};
use workflow_service::store::{
    Collection, MemoryStore, RunArchive, RunColumns, RunRetention, Store, Tracked,
};
use workflow_service::{ApiError, Archive, RunRecord, Workspace};

#[derive(Default)]
struct Runs(RefCell<HashMap<String, (String, u64)>>);
impl RunStore for Runs {
    fn load(&self, run: &str) -> Result<Option<(RunRecord, u64)>, ApiError> {
        Ok(self
            .0
            .borrow()
            .get(run)
            .map(|(body, version)| (serde_json::from_str(body).unwrap(), *version)))
    }
    fn save(&self, run: &str, record: &RunRecord, expected: Option<u64>) -> Result<u64, SaveError> {
        let mut runs = self.0.borrow_mut();
        if runs.get(run).map(|(_, v)| *v) != expected {
            return Err(SaveError::Conflict);
        }
        let version = expected.unwrap_or(0) + 1;
        runs.insert(
            run.into(),
            (serde_json::to_string(record).unwrap(), version),
        );
        Ok(version)
    }
}
/// A run object's alarm: the latest scheduled message wins.
#[derive(Default)]
struct Alarm(RefCell<Option<(u64, WorkMessage)>>, RefCell<u64>);
impl WorkBus for Alarm {
    fn send(&self, message: WorkMessage, delay_ms: Option<u64>) -> Result<(), ApiError> {
        *self.0.borrow_mut() = Some((*self.1.borrow() + delay_ms.unwrap_or(0), message));
        Ok(())
    }
}

fn workflow(id: &str, extra: Value) -> Value {
    let mut nodes = vec![json!({"id":"start","type":"Manual Trigger","label":"Start","config":{}})];
    let mut edges = vec![];
    if !extra.is_null() {
        nodes.push(extra);
        edges.push(json!({"id":"e","source":"start","target":"x"}));
    }
    json!({"schemaVersion":1,"id":id,"name":id,"status":"draft","updatedAt":"","nodes":nodes,"edges":edges})
}
fn handle(ws: &mut Workspace, method: &str, path: &str, body: Value) -> Result<Value, ApiError> {
    let body = if body.is_null() {
        vec![]
    } else {
        body.to_string().into_bytes()
    };
    ws.handle(method, path, &body, 1, &format!("n{}", path.len()))
        .map(|(_, v)| v)
}
fn start(ws: &mut Workspace, workflow: &str, id: &str) -> Result<Value, ApiError> {
    let body = json!({"workflowId":workflow,"mode":"run","input":{}}).to_string();
    ws.handle("POST", "/api/runs", body.as_bytes(), 1, id)
        .map(|(_, v)| v)
}
/// Commit, reload with an archive and the offload setting, as the host does.
fn cycle(store: &mut MemoryStore, tracked: &mut Tracked) -> (Tracked, Vec<RunRecord>) {
    let written = tracked.commit(store).unwrap();
    let started = written
        .upserts
        .iter()
        .filter(|r| r.remote)
        .map(|r| serde_json::from_str(&r.body).unwrap())
        .collect();
    let mut reloaded = Tracked::load(store).unwrap();
    reloaded.workspace.archive = Archive::new(Rc::new(store.clone()));
    reloaded.workspace.offload = true;
    reloaded.workspace.retention = tracked.workspace.retention;
    reloaded.workspace.max_active_runs = tracked.workspace.max_active_runs;
    (reloaded, started)
}
fn report(store: &mut MemoryStore, runs: &Runs, id: &str) -> bool {
    let (record, version) = runs.load(id).unwrap().unwrap();
    store
        .apply_report(
            id,
            version,
            &serde_json::to_string(&record).unwrap(),
            &RunColumns::of(&record).unwrap(),
        )
        .unwrap()
}
/// Run a remote run to completion as its run object would, reporting each save.
fn execute(store: &mut MemoryStore, record: RunRecord) {
    let runs = Runs::default();
    let alarm = Alarm::default();
    *alarm.1.borrow_mut() = 1;
    let id = record.id_string().unwrap();
    execution::start(&runs, &alarm, record, 1).unwrap();
    report(store, &runs, &id);
    for _ in 0..100 {
        let Some((at, message)) = alarm.0.borrow_mut().take() else {
            break;
        };
        *alarm.1.borrow_mut() = at;
        let step = execution::begin(&runs, &alarm, &message, at).unwrap();
        if let Step::Perform { run, task } = step {
            let outcome: Outcome = perform(&task);
            execution::finish(&runs, &alarm, &run, &task.id, outcome, at).unwrap();
        }
        report(store, &runs, &id);
    }
}
fn perform(_task: &Task) -> Outcome {
    Outcome::success(json!({"status":200,"headers":{},"body":{"ok":true}}))
}
fn status(ws: &Workspace, id: &str) -> Value {
    ws.list_runs(100, None, None)
        .unwrap()
        .into_iter()
        .find(|r| r["id"] == id)
        .map(|r| r["status"].clone())
        .unwrap_or(Value::Null)
}

#[test]
fn offloaded_runs_are_indexed_not_held_in_memory() {
    let mut store = MemoryStore::default();
    let mut tracked = Tracked::load(&store).unwrap();
    tracked.workspace.offload = true;
    let http = json!({"id":"x","type":"HTTP Request","label":"Fetch","config":{"url":"https://fixture.example/a"}});
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/fetch",
        workflow("fetch", http),
    )
    .unwrap();
    handle(
        &mut tracked.workspace,
        "POST",
        "/api/workflows/fetch/publish",
        Value::Null,
    )
    .unwrap();
    let (mut tracked, _) = cycle(&mut store, &mut tracked);

    let view = start(&mut tracked.workspace, "fetch", "remote-1").unwrap();
    assert_eq!(view["status"], "running");
    assert!(
        tracked.workspace.runs.is_empty(),
        "never executed in the workspace"
    );
    let (tracked, started) = cycle(&mut store, &mut tracked);
    assert_eq!(started.len(), 1, "handed to the host to start");
    assert!(tracked.workspace.runs.is_empty(), "never loaded");
    assert_eq!(status(&tracked.workspace, "remote-1"), "running", "indexed");

    // The run object executes it and reports; the index follows.
    execute(&mut store, started.into_iter().next().unwrap());
    let tracked = Tracked::load(&store).unwrap();
    assert!(tracked.workspace.runs.is_empty());
    let mut ws = tracked.workspace;
    ws.archive = Archive::new(Rc::new(store.clone()));
    assert_eq!(status(&ws, "remote-1"), "succeeded");
    let detail = handle(&mut ws, "GET", "/api/runs/remote-1", Value::Null).unwrap();
    assert_eq!(
        detail["nodes"][1]["output"]["body"]["ok"], true,
        "details come from the index"
    );
}

#[test]
fn reports_apply_in_version_order_only() {
    let mut store = MemoryStore::default();
    let mut tracked = Tracked::load(&store).unwrap();
    tracked.workspace.offload = true;
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/w",
        workflow(
            "w",
            json!({"id":"x","type":"Wait / Delay","label":"Wait","config":{"delayMs":60000}}),
        ),
    )
    .unwrap();
    handle(
        &mut tracked.workspace,
        "POST",
        "/api/workflows/w/publish",
        Value::Null,
    )
    .unwrap();
    start(&mut tracked.workspace, "w", "r").unwrap();
    let (_, started) = cycle(&mut store, &mut tracked);
    let record = started.into_iter().next().unwrap();
    let mut finished = record.clone();
    finished.control("cancel", 2).unwrap();
    let body = |r: &RunRecord| serde_json::to_string(r).unwrap();
    let columns = |r: &RunRecord| RunColumns::of(r).unwrap();
    assert!(
        store
            .apply_report("r", 5, &body(&finished), &columns(&finished))
            .unwrap()
    );
    // An older report arriving late, or the same one again, changes nothing.
    assert!(
        !store
            .apply_report("r", 3, &body(&record), &columns(&record))
            .unwrap()
    );
    assert!(
        !store
            .apply_report("r", 5, &body(&record), &columns(&record))
            .unwrap()
    );
    assert_eq!(
        store.list_runs(10, None, None).unwrap()[0]["status"],
        "cancelled"
    );
    // Reports only ever update remote runs' index rows.
    assert!(
        !store
            .apply_report("missing", 9, &body(&record), &columns(&record))
            .unwrap()
    );
}

#[test]
fn admission_counts_remote_runs_and_pruning_spares_active_ones() {
    let mut store = MemoryStore::default();
    let mut tracked = Tracked::load(&store).unwrap();
    tracked.workspace.offload = true;
    tracked.workspace.max_active_runs = Some(2);
    tracked.workspace.retention = RunRetention {
        per_workflow: Some(1),
        ..RunRetention::default()
    };
    let wait = json!({"id":"x","type":"Wait / Delay","label":"Wait","config":{"delayMs":60000}});
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/hold",
        workflow("hold", wait),
    )
    .unwrap();
    handle(
        &mut tracked.workspace,
        "POST",
        "/api/workflows/hold/publish",
        Value::Null,
    )
    .unwrap();
    let (mut tracked, _) = cycle(&mut store, &mut tracked);
    start(&mut tracked.workspace, "hold", "a").unwrap();
    // Admission sees runs started earlier in the same operation...
    start(&mut tracked.workspace, "hold", "b").unwrap();
    assert_eq!(
        start(&mut tracked.workspace, "hold", "c")
            .unwrap_err()
            .status,
        429
    );
    let (mut tracked, started) = cycle(&mut store, &mut tracked);
    for record in started {
        let id = record.id_string().unwrap();
        let runs = Runs::default();
        execution::start(&runs, &Alarm::default(), record, 1).unwrap();
        report(&mut store, &runs, &id);
    }
    // ...and remote runs reported as running in later ones.
    let mut ws = Tracked::load(&store).unwrap().workspace;
    ws.archive = Archive::new(Rc::new(store.clone()));
    ws.max_active_runs = Some(2);
    assert_eq!(start(&mut ws, "hold", "d").unwrap_err().status, 429);
    // Retention keeps one run per workflow, but never deletes active ones.
    tracked
        .workspace
        .record_audit("browser", "touch", "", "succeeded", 2, "");
    store
        .prune_runs(&tracked.workspace.retention, &["hold".into()])
        .unwrap();
    assert_eq!(store.list_runs(10, None, None).unwrap().len(), 2);
    assert_eq!(store.remote_active().unwrap(), 2);
    let _ = &mut tracked;
}

#[test]
fn ineligible_workflows_still_run_in_the_workspace() {
    let mut store = MemoryStore::default();
    let mut tracked = Tracked::load(&store).unwrap();
    tracked.workspace.offload = true;
    let respond = json!({"id":"x","type":"Context Store","label":"Read","config":{"command":{"operation":"get","tableId":"cache","key":"k"}}});
    handle(
        &mut tracked.workspace,
        "PUT",
        "/api/workflows/reply",
        workflow("reply", respond),
    )
    .unwrap();
    handle(
        &mut tracked.workspace,
        "POST",
        "/api/workflows/reply/publish",
        Value::Null,
    )
    .unwrap();
    start(&mut tracked.workspace, "reply", "local").unwrap();
    let (_, started) = cycle(&mut store, &mut tracked);
    assert!(started.is_empty());
    assert!(
        store
            .record(&workflow_service::store::EntityKey {
                collection: Collection::Run,
                id: "local".into()
            })
            .unwrap()
            .is_some_and(|r| !r.remote)
    );
}
