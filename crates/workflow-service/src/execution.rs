//! Queue-driven run execution (stage 2 of `docs/workspace-storage.md`).
//!
//! A run tree (a top-level run and the runs it invokes, see [`RunTree`]) advances
//! one message at a time, outside the workspace: a consumer loads the tree from a
//! [`RunStore`], steps it with the workspace's own logic, and sends the next
//! message on a [`WorkBus`]. Both are traits, so Cloudflare (Queues and a
//! Durable Object per run) and other hosts (SQS, NATS, Postgres) can provide them.
//!
//! Delivery is at least once and may be out of order. Correctness comes from the
//! store's compare-and-swap on a version, not from the bus:
//!
//! - every message names the tree version it was sent for, and a consumer acts
//!   only on a message for the current version, so duplicates and stale messages
//!   are acknowledged and dropped;
//! - an HTTP task is claimed (marked dispatched) with compare-and-swap before any
//!   network I/O, so only one consumer performs it; its outcome is applied with
//!   compare-and-swap too, and only to the task that was claimed;
//! - every save that leaves work to do sends exactly one message for the new
//!   version, so the run always has one live message.
//!
//! Network I/O happens between [`begin`] and [`finish`], in the host, so this
//! module stays synchronous and host-independent.
use crate::{ApiError, Mode, RunRecord, WebhookReply, Workspace, error};
use connector_sdk::{FailureCode, Outcome, Task};
use serde::{Deserialize, Serialize};
pub use workflow_engine::RunStatus;

/// A top-level run and every run it invoked (Execute Workflow and Each children,
/// and theirs), stored and stepped together. A tree is the unit of execution: the
/// workspace's own invocation logic runs over it, so children behave exactly as
/// they do in the workspace, and one version covers the whole tree.
///
/// `runs` is in workspace order, newest first, so the root (created first) is
/// last.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunTree {
    pub runs: Vec<RunRecord>,
}
impl RunTree {
    pub fn new(runs: Vec<RunRecord>) -> Self {
        Self { runs }
    }
    /// The tree's top-level run.
    pub fn root(&self) -> Result<&RunRecord, ApiError> {
        self.runs
            .iter()
            .find(|r| r.parent.is_none())
            .ok_or_else(|| error(500, "Run tree has no root"))
    }
    pub fn root_id(&self) -> Result<String, ApiError> {
        self.root()?.id_string()
    }
    /// Whether any run in the tree is still running or paused, or holds an open
    /// webhook response window (its response not yet handed to the request).
    pub fn active(&self) -> Result<bool, ApiError> {
        for run in &self.runs {
            if matches!(run.status()?, RunStatus::Running | RunStatus::Paused)
                || run.webhook_response.as_ref().is_some_and(|w| w.open())
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    /// The run with this ID, if it is in the tree.
    pub fn run(&self, id: &str) -> Option<&RunRecord> {
        self.runs.iter().find(|r| r.matches_id(id))
    }
    /// The workspace's invocation and stepping logic, over this tree alone.
    fn workspace(self) -> Workspace {
        Workspace {
            runs: self.runs,
            ..Workspace::default()
        }
    }
    fn from_workspace(workspace: Workspace) -> Self {
        Self {
            runs: workspace.runs,
        }
    }
}

/// "This run tree has work", for the version it was sent for. Messages carry no
/// data: consumers always re-read the tree. `run` is the root's ID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkMessage {
    pub run: String,
    pub version: u64,
}

/// A message bus with at-least-once delivery and optional delay.
pub trait WorkBus {
    fn send(&self, message: WorkMessage, delay_ms: Option<u64>) -> Result<(), ApiError>;
}

/// Why a save was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum SaveError {
    /// The tree changed since it was loaded: another consumer owns this step.
    Conflict,
    Failed(String),
}

/// Durable run trees with compare-and-swap on a version, keyed by root ID.
pub trait RunStore {
    /// The tree and its version, or `None` if it doesn't exist.
    fn load(&self, root: &str) -> Result<Option<(RunTree, u64)>, ApiError>;
    /// Save if the stored version is still `expected` (`None` to create), and
    /// return the new version.
    fn save(&self, root: &str, tree: &RunTree, expected: Option<u64>) -> Result<u64, SaveError>;
}

/// What a consumer must do after [`begin`].
#[derive(Debug, PartialEq)]
pub enum Step {
    /// Nothing to do: the message was stale or a duplicate, the tree is waiting,
    /// or it has finished. Acknowledge the message.
    Done(Progress),
    /// Perform this task's network I/O (with its idempotency key), then call
    /// [`finish`] with the outcome. `run` is the run in the tree that owns it.
    Perform { run: String, task: Task },
    /// Apply this data task (Data Tables or Context Store) to the workspace's
    /// data with [`perform_data`] at `time`, then call [`finish_data`]. It isn't
    /// claimed: the effect is keyed by the task's idempotency key, so performing
    /// it again (after a lost outcome) replays the same result.
    Data { run: String, task: Task, time: u64 },
}

/// If a data task's outcome doesn't arrive (the host failed after `begin`), the
/// tree is stepped again after this long and the task performed again.
pub const DATA_RETRY_MS: u64 = 10_000;

/// Apply a data task's effect to `store`, as the workspace does. `Err` is a
/// storage failure: the host must roll back and try again later. Any other
/// failure is the task's outcome; the host must roll back the effect (its
/// transaction) before recording it.
pub fn perform_data(
    store: &mut impl workflow_data::Store,
    task: &Task,
    time: u64,
) -> Result<Outcome, workflow_data::DataError> {
    if time >= task.deadline_ms {
        return Ok(connectors_http::failure(FailureCode::Timeout, false));
    }
    let prepared = match connectors_data::prepare(&task.node, &task.input) {
        Ok(prepared) => prepared,
        Err(code) => return Ok(connectors_http::failure(code, false)),
    };
    match connectors_data::apply(store, prepared, &task.idempotency_key, time) {
        Ok(value) => Ok(Outcome::success(value)),
        Err(error) if error.code == workflow_data::ErrorCode::Storage => Err(error),
        Err(_) => Ok(connectors_http::failure(FailureCode::InvalidConfig, false)),
    }
}

/// Status changes for the workspace's run index.
#[derive(Debug, PartialEq)]
pub enum Progress {
    /// Nothing changed that the index tracks.
    Unchanged,
    /// The tree was saved at this version; `status` is its root's.
    Saved { version: u64, status: RunStatus },
}

/// Whether a run can execute outside the workspace. Everything can, except
/// simulations that use data operations (directly or in a workflow they may
/// invoke): those run against a snapshot of the workspace's data taken when they
/// start, which stays with the workspace.
pub fn eligible(record: &RunRecord) -> Result<bool, ApiError> {
    if record.mode != Mode::Simulation {
        return Ok(true);
    }
    let run = record.restore()?;
    Ok(std::iter::once(run.workflow())
        .chain(record.invocation_targets.values())
        .all(|graph| {
            !graph
                .nodes
                .iter()
                .any(|n| connectors_data::handles(&n.kind))
        }))
}

/// Create a tree's first state in the store and send its first message. The
/// root may be new (it is advanced here) or already advanced, with children.
pub fn start(
    store: &dyn RunStore,
    bus: &dyn WorkBus,
    tree: RunTree,
    now: u64,
) -> Result<Progress, ApiError> {
    let root = tree.root_id()?;
    let mut workspace = tree.workspace();
    let index = workspace
        .run_index(&root)
        .ok_or_else(|| error(500, "Run tree has no root"))?;
    workspace.runs[index].advance(now)?;
    workspace.settle_invocations(now)?;
    let tree = RunTree::from_workspace(workspace);
    let version = match store.save(&root, &tree, None) {
        Ok(version) => version,
        Err(SaveError::Conflict) => return Err(error(409, "Run already exists")),
        Err(SaveError::Failed(message)) => return Err(error(500, message)),
    };
    schedule(bus, &tree, &root, version, now)?;
    Ok(Progress::Saved {
        version,
        status: tree.root()?.status()?,
    })
}

/// Process one message: step the tree's due runs and save it, or claim an HTTP
/// task for the host to perform.
pub fn begin(
    store: &dyn RunStore,
    bus: &dyn WorkBus,
    message: &WorkMessage,
    now: u64,
) -> Result<Step, ApiError> {
    let Some((tree, version)) = store.load(&message.run)? else {
        return Ok(Step::Done(Progress::Unchanged));
    };
    // Only the message for the current version may act. Older ones were
    // superseded by a later save, which sent its own message.
    if version != message.version || !running(&tree)? {
        return Ok(Step::Done(Progress::Unchanged));
    }
    if next_wake(&tree)?.is_some_and(|at| at > now) && !claimable(&tree, now)? {
        // Delivered early (or a duplicate of a delayed message): wait again.
        schedule(bus, &tree, &message.run, version, now)?;
        return Ok(Step::Done(Progress::Unchanged));
    }
    let mut workspace = tree.workspace();
    workspace.alarm(now)?;
    // Data tasks first, as the workspace performs them after every save.
    if let Some((run, task, _, time)) = workspace.pending_data(now)? {
        let tree = RunTree::from_workspace(workspace);
        return match store.save(&message.run, &tree, Some(version)) {
            Ok(saved) => {
                // The live message, in case the outcome never arrives. The
                // outcome's save supersedes it.
                bus.send(
                    WorkMessage {
                        run: message.run.clone(),
                        version: saved,
                    },
                    Some(DATA_RETRY_MS),
                )?;
                Ok(Step::Data { run, task, time })
            }
            Err(SaveError::Conflict) => Ok(Step::Done(Progress::Unchanged)),
            Err(SaveError::Failed(message)) => Err(error(500, message)),
        };
    }
    if let Some((run, task)) = workspace.claim_external(now)? {
        let tree = RunTree::from_workspace(workspace);
        // Persist the claim before any network I/O: if this fails, another
        // consumer owns the step.
        return match store.save(&message.run, &tree, Some(version)) {
            Ok(claimed) => {
                // The next message claims the next ready task straight away, so
                // parallel requests are in flight together. Once none is left, it
                // waits for the earliest claimed deadline: if the host never
                // reports back, that settles the task as "outcome unknown", as in
                // the workspace. `finish` supersedes it by saving a newer version.
                schedule(bus, &tree, &message.run, claimed, now)?;
                Ok(Step::Perform { run, task })
            }
            Err(SaveError::Conflict) => Ok(Step::Done(Progress::Unchanged)),
            Err(SaveError::Failed(message)) => Err(error(500, message)),
        };
    }
    save_and_schedule(
        store,
        bus,
        &message.run,
        &RunTree::from_workspace(workspace),
        version,
        now,
    )
}

/// Apply a performed task's outcome to the run in tree `root` that claimed it,
/// step the tree, save it and send its next message. A stale outcome (the run
/// moved on, or the task was never claimed) is dropped.
pub fn finish(
    store: &dyn RunStore,
    bus: &dyn WorkBus,
    root: &str,
    run: &str,
    task_id: &str,
    outcome: Outcome,
    now: u64,
) -> Result<Progress, ApiError> {
    let Some((tree, version)) = store.load(root)? else {
        return Ok(Progress::Unchanged);
    };
    let mut workspace = tree.workspace();
    if !workspace.complete_external(run, task_id, outcome, now)? {
        return Ok(Progress::Unchanged);
    }
    match save_and_schedule(
        store,
        bus,
        root,
        &RunTree::from_workspace(workspace),
        version,
        now,
    )? {
        Step::Done(progress) => Ok(progress),
        _ => unreachable!("save_and_schedule never claims"),
    }
}

/// Apply a data task's outcome (from [`perform_data`] at `time`) to the run in
/// tree `root`, step the tree, save it and send its next message. An outcome for
/// a task that is no longer pending (applied already) is dropped.
#[allow(clippy::too_many_arguments)]
pub fn finish_data(
    store: &dyn RunStore,
    bus: &dyn WorkBus,
    root: &str,
    run: &str,
    task_id: &str,
    outcome: Outcome,
    time: u64,
    now: u64,
) -> Result<Progress, ApiError> {
    let Some((tree, version)) = store.load(root)? else {
        return Ok(Progress::Unchanged);
    };
    let mut workspace = tree.workspace();
    if !workspace.complete_data(run, task_id, outcome, time)? {
        return Ok(Progress::Unchanged);
    }
    match save_and_schedule(
        store,
        bus,
        root,
        &RunTree::from_workspace(workspace),
        version,
        now,
    )? {
        Step::Done(progress) => Ok(progress),
        _ => unreachable!("save_and_schedule never claims"),
    }
}

/// Answer the handler holding run `run`'s webhook request, as the workspace
/// does ([`Workspace::webhook_reply`]): mark a ready response submitted (or the
/// window closed, if the client disconnected) and save that before it is sent.
/// `None` if the tree isn't stored here.
pub fn reply(
    store: &dyn RunStore,
    bus: &dyn WorkBus,
    root: &str,
    run: &str,
    now: u64,
    disconnected: bool,
) -> Result<Option<WebhookReply>, ApiError> {
    for _ in 0..8 {
        let Some((tree, version)) = store.load(root)? else {
            return Ok(None);
        };
        let mut workspace = tree.workspace();
        let reply = workspace.webhook_reply(run, now, disconnected)?;
        if reply == WebhookReply::Pending {
            // Nothing changed: the window is still waiting for its response.
            return Ok(Some(reply));
        }
        let tree = RunTree::from_workspace(workspace);
        match store.save(root, &tree, Some(version)) {
            Ok(saved) => {
                schedule(bus, &tree, root, saved, now)?;
                return Ok(Some(reply));
            }
            Err(SaveError::Conflict) => continue,
            Err(SaveError::Failed(message)) => return Err(error(500, message)),
        }
    }
    Err(error(409, "The run is changing too quickly; try again"))
}

/// Whether the tree needs messages: a run is running (paused runs wait for a
/// control request), or holds an open response window, which expires at its
/// deadline as in the workspace.
fn running(tree: &RunTree) -> Result<bool, ApiError> {
    for run in &tree.runs {
        if run.status()? == RunStatus::Running || open_window(run).is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}
/// An open response window's deadline (`u64::MAX` if it has none).
fn open_window(run: &RunRecord) -> Option<u64> {
    run.webhook_response
        .as_ref()
        .filter(|w| w.open())
        .map(|w| w.deadline_ms.unwrap_or(u64::MAX))
}
/// The earliest time the tree needs the engine: a running run's wake, or an open
/// window's deadline.
fn next_wake(tree: &RunTree) -> Result<Option<u64>, ApiError> {
    let mut next = None;
    for run in &tree.runs {
        let wake = if run.status()? == RunStatus::Running {
            run.wake_at
        } else {
            None
        };
        for at in wake.into_iter().chain(open_window(run)) {
            next = Some(next.map_or(at, |n: u64| n.min(at)));
        }
    }
    Ok(next)
}
/// Whether a running run has work to hand the host now: an HTTP task ready to
/// claim (as `claim_external` would), or a data task (performed straight after
/// the save that created it, as the workspace does).
fn claimable(tree: &RunTree, now: u64) -> Result<bool, ApiError> {
    for run in &tree.runs {
        if run.status()? == RunStatus::Running
            && run
                .external
                .iter()
                .chain(run.pending.iter())
                .any(|e| e.performable(now) || connectors_data::handles(&e.task.node.kind))
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn save_and_schedule(
    store: &dyn RunStore,
    bus: &dyn WorkBus,
    root: &str,
    tree: &RunTree,
    version: u64,
    now: u64,
) -> Result<Step, ApiError> {
    let version = match store.save(root, tree, Some(version)) {
        Ok(version) => version,
        Err(SaveError::Conflict) => return Ok(Step::Done(Progress::Unchanged)),
        Err(SaveError::Failed(message)) => return Err(error(500, message)),
    };
    schedule(bus, tree, root, version, now)?;
    Ok(Step::Done(Progress::Saved {
        version,
        status: tree.root()?.status()?,
    }))
}
/// Send the tree's next message for `version`, if it has more work: now, or when
/// it next needs the engine.
fn schedule(
    bus: &dyn WorkBus,
    tree: &RunTree,
    root: &str,
    version: u64,
    now: u64,
) -> Result<(), ApiError> {
    if !running(tree)? {
        return Ok(());
    }
    let delay = if claimable(tree, now)? {
        None
    } else {
        match next_wake(tree)? {
            Some(at) if at > now => Some(at - now),
            // Due, or running with nothing scheduled: step again now.
            _ => None,
        }
    };
    bus.send(
        WorkMessage {
            run: root.into(),
            version,
        },
        delay,
    )
}

/// Pause, resume or cancel a run in tree `root`, with compare-and-swap (retried
/// if a step saves in between), then schedule the tree's next message. Returns
/// the run as saved.
pub fn control(
    store: &dyn RunStore,
    bus: &dyn WorkBus,
    root: &str,
    run: &str,
    action: &str,
    now: u64,
) -> Result<(RunRecord, Progress), ApiError> {
    for _ in 0..8 {
        let Some((tree, version)) = store.load(root)? else {
            return Err(error(404, "Run not found"));
        };
        let mut workspace = tree.workspace();
        let index = workspace
            .run_index(run)
            .ok_or_else(|| error(404, "Run not found"))?;
        workspace.runs[index].control(action, now)?;
        workspace.settle_invocations(now)?;
        let tree = RunTree::from_workspace(workspace);
        match store.save(root, &tree, Some(version)) {
            Ok(saved) => {
                schedule(bus, &tree, root, saved, now)?;
                let status = tree.root()?.status()?;
                let record = tree.run(run).cloned().unwrap();
                return Ok((
                    record,
                    Progress::Saved {
                        version: saved,
                        status,
                    },
                ));
            }
            Err(SaveError::Conflict) => continue,
            Err(SaveError::Failed(message)) => return Err(error(500, message)),
        }
    }
    Err(error(409, "The run is changing too quickly; try again"))
}
