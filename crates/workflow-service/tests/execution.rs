//! Queue-driven execution must produce exactly the runs the workspace produces,
//! however messages are delivered: duplicated, reordered, or interrupted by a
//! consumer that crashes mid-step.
use connector_sdk::{Outcome, Task};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::{fs, path::PathBuf};
use workflow_service::execution::{
    self, Progress, RunStatus, RunStore, RunTree, SaveError, Step, WorkBus, WorkMessage,
};
use workflow_service::{ApiError, Mode, RunRecord, Workspace};

/// Run trees stored as JSON, as a real store would, with a version per tree.
#[derive(Default)]
struct MemoryRuns(RefCell<HashMap<String, (String, u64)>>);
impl RunStore for MemoryRuns {
    fn load(&self, run: &str) -> Result<Option<(RunTree, u64)>, ApiError> {
        Ok(self
            .0
            .borrow()
            .get(run)
            .map(|(body, version)| (serde_json::from_str(body).unwrap(), *version)))
    }
    fn save(&self, run: &str, record: &RunTree, expected: Option<u64>) -> Result<u64, SaveError> {
        let mut runs = self.0.borrow_mut();
        let current = runs.get(run).map(|(_, v)| *v);
        if current != expected {
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

/// Messages due at a time. The driver decides how they're delivered.
#[derive(Default)]
struct MemoryBus {
    queue: RefCell<Vec<(u64, WorkMessage)>>,
    now: Cell<u64>,
    sent: Cell<usize>,
}
impl WorkBus for MemoryBus {
    fn send(&self, message: WorkMessage, delay_ms: Option<u64>) -> Result<(), ApiError> {
        self.sent.set(self.sent.get() + 1);
        let at = self.now.get() + delay_ms.unwrap_or(0);
        self.queue.borrow_mut().push((at, message));
        Ok(())
    }
}

/// The same deterministic outcome for an HTTP task, whoever performs it.
fn perform(task: &Task) -> Outcome {
    Outcome::success(
        json!({"status": 200, "headers": {}, "body": {"node": task.node.id, "input": task.input}}),
    )
}

/// How the driver interferes with delivery.
#[derive(Clone, Copy, Debug)]
enum Chaos {
    None,
    /// Every message is delivered twice.
    Duplicate,
    /// Messages due at the same time are delivered in a scrambled order.
    Shuffle(u64),
    /// The first claimed task's consumer dies before reporting back.
    Crash,
}

/// Every run of a tree as the API shows it, by ID.
fn views<'a>(runs: impl Iterator<Item = &'a RunRecord>) -> Vec<Value> {
    let mut views: Vec<Value> = runs.map(|r| r.view(false).unwrap()).collect();
    views.sort_by_key(|v| v["id"].as_str().unwrap().to_owned());
    views
}
fn tree(record: RunRecord) -> RunTree {
    RunTree::new(vec![record])
}

fn queue_run(ws: &Workspace, workflow: &str, mode: Mode, chaos: Chaos) -> Vec<Value> {
    let store = MemoryRuns::default();
    let bus = MemoryBus::default();
    let doc = ws.live_document(workflow).unwrap();
    let record = ws
        .new_run_record(
            "queued",
            &doc,
            json!({"from": "test"}),
            mode,
            0,
            json!({"trigger": "manual"}),
        )
        .unwrap();
    execution::start(&store, &bus, tree(record), 0).unwrap();
    let mut crashed = false;
    let mut seed = match chaos {
        Chaos::Shuffle(seed) => seed,
        _ => 1,
    };
    for _ in 0..10_000 {
        let next = {
            let mut queue = bus.queue.borrow_mut();
            if queue.is_empty() {
                break;
            }
            let earliest = queue.iter().map(|(at, _)| *at).min().unwrap();
            let due: Vec<usize> = (0..queue.len())
                .filter(|i| queue[*i].0 == earliest)
                .collect();
            let pick = if let Chaos::Shuffle(_) = chaos {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                due[(seed >> 33) as usize % due.len()]
            } else {
                due[0]
            };
            queue.remove(pick)
        };
        let (at, message) = next;
        bus.now.set(bus.now.get().max(at));
        let deliveries = if matches!(chaos, Chaos::Duplicate) {
            2
        } else {
            1
        };
        for _ in 0..deliveries {
            let now = bus.now.get();
            match execution::begin(&store, &bus, &message, now).unwrap() {
                Step::Done(_) => {}
                Step::Perform { run, task } => {
                    if matches!(chaos, Chaos::Crash) && !crashed {
                        crashed = true;
                        continue;
                    }
                    execution::finish(
                        &store,
                        &bus,
                        &message.run,
                        &run,
                        &task.id,
                        perform(&task),
                        now,
                    )
                    .unwrap();
                }
            }
        }
    }
    assert!(bus.queue.borrow().is_empty(), "the run settled");
    let (tree, _) = store.load("queued").unwrap().unwrap();
    views(tree.runs.iter())
}

/// The reference: the same run executed by the workspace.
fn workspace_run(ws: &mut Workspace, workflow: &str, mode: Mode, crash: bool) -> Vec<Value> {
    let doc = ws.live_document(workflow).unwrap();
    let record = ws
        .new_run_record(
            "queued",
            &doc,
            json!({"from": "test"}),
            mode,
            0,
            json!({"trigger": "manual"}),
        )
        .unwrap();
    ws.start_record(record, 0).unwrap();
    let mut now = 0;
    let mut crashed = false;
    for _ in 0..10_000 {
        while let Some((run, task)) = ws.claim_external(now).unwrap() {
            if crash && !crashed {
                crashed = true;
                continue;
            }
            ws.complete_external(&run, &task.id, perform(&task), now)
                .unwrap();
        }
        // Until every run of the tree, background children included, settles.
        if !ws
            .runs
            .iter()
            .any(|r| r.status().unwrap() == RunStatus::Running)
        {
            return views(ws.runs.iter());
        }
        match ws.next_alarm() {
            Some(at) => {
                now = now.max(at);
                ws.alarm(now).unwrap();
            }
            None => return views(ws.runs.iter()),
        }
    }
    panic!("the workspace run never settled");
}

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}
fn read(path: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(examples().join(path)).unwrap()).unwrap()
}
fn publish(ws: &mut Workspace, doc: &Value) {
    let id = doc["id"].as_str().unwrap();
    for (method, path, body) in [
        ("PUT", format!("/api/workflows/{id}"), doc.to_string()),
        (
            "POST",
            format!("/api/workflows/{id}/publish"),
            String::new(),
        ),
    ] {
        ws.handle(method, &path, body.as_bytes(), 0, "").unwrap();
    }
}

/// A workspace with every case published, those invoking others after their
/// targets.
fn published() -> Workspace {
    let mut ws = Workspace::default();
    let mut remaining = cases();
    while !remaining.is_empty() {
        let before = remaining.len();
        remaining.retain(|doc| {
            let id = doc["id"].as_str().unwrap();
            let mut trial = ws.clone();
            let ok = [
                ("PUT", format!("/api/workflows/{id}"), doc.to_string()),
                (
                    "POST",
                    format!("/api/workflows/{id}/publish"),
                    String::new(),
                ),
            ]
            .into_iter()
            .all(|(method, path, body)| {
                trial.handle(method, &path, body.as_bytes(), 0, "").is_ok()
            });
            if ok {
                ws = trial;
            }
            !ok
        });
        if remaining.len() == before {
            break;
        }
    }
    ws
}

/// Every published example that can run on queues, plus workflows exercising
/// HTTP, waits, parallel branches and child workflows.
fn cases() -> Vec<Value> {
    let mut cases: Vec<Value> = read("scenarios.json")
        .as_array()
        .unwrap()
        .iter()
        .map(|case| read(case["file"].as_str().unwrap()))
        .collect();
    let node = |id: &str, kind: &str, config: Value| json!({"id": id, "type": kind, "label": id, "config": config});
    let edge = |id: &str, source: &str, target: &str| json!({"id": id, "source": source, "target": target});
    cases.push(json!({"schemaVersion":1,"id":"http-chain","name":"HTTP chain","status":"draft","updatedAt":"",
        "nodes":[node("start","Manual Trigger",json!({})),node("first","HTTP Request",json!({"url":"https://fixture.example/a"})),
                 node("pause","Wait / Delay",json!({"delayMs":5000})),node("second","HTTP Request",json!({"url":"https://fixture.example/b"}))],
        "edges":[edge("a","start","first"),edge("b","first","pause"),edge("c","pause","second")]}));
    cases.push(json!({"schemaVersion":1,"id":"fan-out","name":"Fan out","status":"draft","updatedAt":"",
        "nodes":[node("start","Manual Trigger",json!({})),node("split","Split / Parallel",json!({"concurrency":2})),
                 node("left","HTTP Request",json!({"url":"https://fixture.example/l"})),node("right","HTTP Request",json!({"url":"https://fixture.example/r"})),
                 node("join","Join",json!({"policy":"all"}))],
        "edges":[edge("a","start","split"),edge("b","split","left"),edge("c","split","right"),edge("d","left","join"),edge("e","right","join")]}));
    let child = |id: &str, nodes: Vec<Value>, output: &str| {
        let mut all = vec![
            json!({"id":"start","type":"Execute Workflow Trigger","label":"start","config":{"outputNodeId":output}}),
        ];
        let edges: Vec<Value> = std::iter::once("start".to_owned())
            .chain(nodes.iter().map(|n| n["id"].as_str().unwrap().to_owned()))
            .collect::<Vec<_>>()
            .windows(2)
            .enumerate()
            .map(|(i, w)| edge(&format!("e{i}"), &w[0], &w[1]))
            .collect();
        all.extend(nodes);
        json!({"schemaVersion":1,"id":id,"name":id,"status":"draft","updatedAt":"","nodes":all,"edges":edges})
    };
    let parent = |id: &str, last: Value| {
        json!({"schemaVersion":1,"id":id,"name":id,"status":"draft","updatedAt":"",
        "nodes":[node("start","Manual Trigger",json!({})),node("prepare","Map / Transform",json!({"mapping":{"items":[{"n":1},{"n":2},{"n":3},{"n":4},{"n":5}]}})),last],
        "edges":[edge("a","start","prepare"),edge("b","prepare","x")]})
    };
    // A child that waits and calls HTTP; a parent awaiting one, and one starting
    // another in the background; Each over it.
    cases.push(child(
        "http-worker",
        vec![
            node("pause", "Wait / Delay", json!({"delayMs":2000})),
            node(
                "fetch",
                "HTTP Request",
                json!({"url":"https://fixture.example/w"}),
            ),
        ],
        "fetch",
    ));
    cases.push(parent("invoke-http", json!({"id":"x","type":"Execute Workflow","label":"x","config":{"workflowId":"http-worker"}})));
    cases.push(parent("background-http", json!({"id":"x","type":"Execute Workflow","label":"x","config":{"workflowId":"http-worker","mode":"background"}})));
    cases.push(parent("each-http", json!({"id":"x","type":"Each / Loop","label":"x","config":{"workflowId":"http-worker","arrayPointer":"/items","concurrency":2,"failurePolicy":"collect"}})));
    // Workflows that need the workspace, directly or through a child.
    let context = node(
        "x",
        "Context Store",
        json!({"command":{"operation":"get","tableId":"cache","key":"k"}}),
    );
    cases.push(parent("read-context", context.clone()));
    cases.push(child(
        "context-worker",
        vec![node(
            "read",
            "Context Store",
            json!({"command":{"operation":"get","tableId":"cache","key":"k"}}),
        )],
        "read",
    ));
    cases.push(parent("invoke-context", json!({"id":"x","type":"Execute Workflow","label":"x","config":{"workflowId":"context-worker"}})));
    cases.push(json!({"schemaVersion":1,"id":"reply","name":"reply","status":"draft","updatedAt":"",
        "nodes":[node("start","Webhook",json!({})),node("x","Respond to Webhook",json!({"status":200}))],"edges":[edge("a","start","x")]}));
    cases
}

#[test]
fn queued_runs_match_workspace_runs_under_any_delivery() {
    let mut checked = 0;
    let mut invoking = 0;
    let ws = published();
    for doc in cases() {
        let id = doc["id"].as_str().unwrap();
        let Ok(live) = ws.live_document(id) else {
            continue;
        };
        let Ok(record) = ws.new_run_record("probe", &live, json!({}), Mode::Run, 0, json!({}))
        else {
            continue;
        };
        if !execution::eligible(&record).unwrap() {
            continue;
        }
        let invokes = !record.invocation_targets.is_empty();
        invoking += usize::from(invokes);
        for mode in [Mode::Run, Mode::Simulation] {
            let expected = workspace_run(&mut ws.clone(), id, mode, false);
            assert_eq!(
                expected.len() > 1,
                invokes,
                "{id}: children are compared too"
            );
            for chaos in [
                Chaos::None,
                Chaos::Duplicate,
                Chaos::Shuffle(7),
                Chaos::Shuffle(99),
            ] {
                assert_eq!(
                    queue_run(&ws, id, mode, chaos),
                    expected,
                    "{id} {mode:?} {chaos:?}"
                );
            }
            checked += 1;
        }
    }
    assert!(checked >= 20, "only {checked} cases ran");
    assert!(
        invoking >= 3,
        "only {invoking} cases invoke other workflows"
    );
}

#[test]
fn a_crashed_consumer_settles_like_the_workspace() {
    let mut ws = Workspace::default();
    let doc = cases()
        .into_iter()
        .find(|d| d["id"] == "http-chain")
        .unwrap();
    publish(&mut ws, &doc);
    // The claimed request's outcome is never reported; at its deadline the run
    // records "outcome unknown", whichever way it executes.
    let expected = workspace_run(&mut ws.clone(), "http-chain", Mode::Run, true);
    assert_eq!(expected[0]["status"], "failed", "{expected:?}");
    assert_eq!(
        queue_run(&ws, "http-chain", Mode::Run, Chaos::Crash),
        expected
    );
}

#[test]
fn stale_and_duplicate_messages_do_nothing() {
    let mut ws = Workspace::default();
    let doc = cases()
        .into_iter()
        .find(|d| d["id"] == "http-chain")
        .unwrap();
    publish(&mut ws, &doc);
    let store = MemoryRuns::default();
    let bus = MemoryBus::default();
    let record = ws
        .new_run_record(
            "r",
            &ws.live_document("http-chain").unwrap(),
            json!({}),
            Mode::Run,
            0,
            json!({}),
        )
        .unwrap();
    execution::start(&store, &bus, tree(record), 0).unwrap();
    let first = bus.queue.borrow()[0].1.clone();
    let Step::Perform { run, task } = execution::begin(&store, &bus, &first, 0).unwrap() else {
        panic!("the first step claims the request");
    };
    // Redelivering an older version does nothing; the claimed version may step
    // the run (as the workspace's alarm would), but claims nothing more.
    for version in [first.version, 0] {
        let stale = WorkMessage {
            run: run.clone(),
            version,
        };
        assert_eq!(
            execution::begin(&store, &bus, &stale, 1).unwrap(),
            Step::Done(Progress::Unchanged),
            "version {version}"
        );
    }
    let current = WorkMessage {
        run: run.clone(),
        version: first.version + 1,
    };
    assert!(matches!(
        execution::begin(&store, &bus, &current, 1).unwrap(),
        Step::Done(_)
    ));
    // An outcome for a task that wasn't claimed is dropped; the real one applies once.
    assert_eq!(
        execution::finish(&store, &bus, &run, &run, "not-a-task", perform(&task), 1).unwrap(),
        Progress::Unchanged
    );
    assert!(matches!(
        execution::finish(&store, &bus, &run, &run, &task.id, perform(&task), 1).unwrap(),
        Progress::Saved { .. }
    ));
    assert_eq!(
        execution::finish(&store, &bus, &run, &run, &task.id, perform(&task), 1).unwrap(),
        Progress::Unchanged
    );
    // The run now waits 5 s. A stale message delivered once it's due must not
    // advance it: only the message for the current version may.
    let (_, current) = store.load(&run).unwrap().unwrap();
    let sent = bus.sent.get();
    let stale = WorkMessage {
        run: run.clone(),
        version: current - 1,
    };
    assert_eq!(
        execution::begin(&store, &bus, &stale, 10_000).unwrap(),
        Step::Done(Progress::Unchanged)
    );
    assert_eq!(
        store.load(&run).unwrap().unwrap().1,
        current,
        "nothing saved"
    );
    assert_eq!(bus.sent.get(), sent, "nothing sent");
    // Starting the same run twice is refused.
    let again = ws
        .new_run_record(
            "r",
            &ws.live_document("http-chain").unwrap(),
            json!({}),
            Mode::Run,
            0,
            json!({}),
        )
        .unwrap();
    assert_eq!(
        execution::start(&store, &bus, tree(again), 0)
            .unwrap_err()
            .status,
        409
    );
}

#[test]
fn workflows_with_data_or_responses_stay_in_the_workspace() {
    let ws = published();
    let record = |id: &str| {
        ws.new_run_record(
            "e",
            &ws.live_document(id).unwrap(),
            json!({}),
            Mode::Run,
            0,
            json!({}),
        )
        .unwrap()
    };
    // Children and Each run in run stores, with their parents...
    for id in ["fan-out", "invoke-http", "each-http"] {
        assert!(execution::eligible(&record(id)).unwrap(), "{id}");
    }
    // ...unless the run, or a workflow it invokes, needs the workspace.
    for id in ["read-context", "invoke-context", "reply"] {
        assert!(!execution::eligible(&record(id)).unwrap(), "{id}");
    }
}

/// A consumer that read the run before another consumer saved it: its loads see
/// that old state, its saves go to the real store.
struct ReadEarlier<'a> {
    store: &'a MemoryRuns,
    snapshot: (RunTree, u64),
}
impl RunStore for ReadEarlier<'_> {
    fn load(&self, _run: &str) -> Result<Option<(RunTree, u64)>, ApiError> {
        Ok(Some(self.snapshot.clone()))
    }
    fn save(&self, run: &str, record: &RunTree, expected: Option<u64>) -> Result<u64, SaveError> {
        self.store.save(run, record, expected)
    }
}

#[test]
fn racing_consumers_cannot_both_claim_or_complete() {
    let mut ws = Workspace::default();
    let doc = cases()
        .into_iter()
        .find(|d| d["id"] == "http-chain")
        .unwrap();
    publish(&mut ws, &doc);
    let store = MemoryRuns::default();
    let bus = MemoryBus::default();
    let record = ws
        .new_run_record(
            "r",
            &ws.live_document("http-chain").unwrap(),
            json!({}),
            Mode::Run,
            0,
            json!({}),
        )
        .unwrap();
    execution::start(&store, &bus, tree(record), 0).unwrap();
    let message = bus.queue.borrow()[0].1.clone();
    // Both consumers receive the same message; B reads the run, then A claims.
    let late = ReadEarlier {
        store: &store,
        snapshot: store.load("r").unwrap().unwrap(),
    };
    let Step::Perform { task, .. } = execution::begin(&store, &bus, &message, 0).unwrap() else {
        panic!("A claims the request");
    };
    assert_eq!(
        execution::begin(&late, &bus, &message, 0).unwrap(),
        Step::Done(Progress::Unchanged),
        "B's claim conflicts, so the request is performed once"
    );
    // The same holds for outcomes: one read before the other's save is refused.
    let late = ReadEarlier {
        store: &store,
        snapshot: store.load("r").unwrap().unwrap(),
    };
    assert!(matches!(
        execution::finish(&store, &bus, "r", "r", &task.id, perform(&task), 1).unwrap(),
        Progress::Saved { .. }
    ));
    assert_eq!(
        execution::finish(&late, &bus, "r", "r", &task.id, perform(&task), 1).unwrap(),
        Progress::Unchanged
    );
}

#[test]
fn control_requests_apply_to_stored_runs() {
    let mut ws = Workspace::default();
    let doc = cases()
        .into_iter()
        .find(|d| d["id"] == "http-chain")
        .unwrap();
    publish(&mut ws, &doc);
    let store = MemoryRuns::default();
    let bus = MemoryBus::default();
    let record = ws
        .new_run_record(
            "r",
            &ws.live_document("http-chain").unwrap(),
            json!({}),
            Mode::Run,
            0,
            json!({}),
        )
        .unwrap();
    execution::start(&store, &bus, tree(record), 0).unwrap();
    let (paused, _) = execution::control(&store, &bus, "r", "r", "pause", 1).unwrap();
    assert_eq!(paused.view(false).unwrap()["status"], "paused");
    // A paused run's messages do nothing until it resumes.
    let message = WorkMessage {
        run: "r".into(),
        version: store.load("r").unwrap().unwrap().1,
    };
    assert_eq!(
        execution::begin(&store, &bus, &message, 2).unwrap(),
        Step::Done(Progress::Unchanged)
    );
    let (resumed, _) = execution::control(&store, &bus, "r", "r", "resume", 3).unwrap();
    assert_eq!(resumed.view(false).unwrap()["status"], "running");
    let (cancelled, _) = execution::control(&store, &bus, "r", "r", "cancel", 4).unwrap();
    assert_eq!(cancelled.view(false).unwrap()["status"], "cancelled");
    assert_eq!(
        execution::control(&store, &bus, "r", "r", "resume", 5)
            .unwrap_err()
            .status,
        409
    );
    assert_eq!(
        execution::control(&store, &bus, "missing", "missing", "cancel", 5)
            .unwrap_err()
            .status,
        404
    );
}

#[test]
fn parallel_requests_are_claimed_together() {
    let mut ws = Workspace::default();
    let doc = cases().into_iter().find(|d| d["id"] == "fan-out").unwrap();
    publish(&mut ws, &doc);
    let store = MemoryRuns::default();
    let bus = MemoryBus::default();
    let record = ws
        .new_run_record(
            "p",
            &ws.live_document("fan-out").unwrap(),
            json!({}),
            Mode::Run,
            0,
            json!({}),
        )
        .unwrap();
    execution::start(&store, &bus, tree(record), 0).unwrap();
    // Each claim's follow-up message is due now while another request is ready,
    // so both requests are in flight before either reports back (a rendezvous
    // between them would otherwise deadlock until a deadline).
    let mut claimed = vec![];
    for _ in 0..2 {
        let (at, message) = bus.queue.borrow_mut().remove(0);
        assert_eq!(at, 0, "due immediately");
        let Step::Perform { task, .. } = execution::begin(&store, &bus, &message, 0).unwrap()
        else {
            panic!("claims a request");
        };
        claimed.push(task.node.id);
    }
    claimed.sort();
    assert_eq!(claimed, ["left", "right"]);
    // With nothing left to claim, the next message waits for a deadline.
    assert!(bus.queue.borrow()[0].0 > 0);
}
