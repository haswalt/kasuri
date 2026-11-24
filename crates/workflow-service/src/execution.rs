//! Queue-driven run execution (stage 2 of `docs/workspace-storage.md`).
//!
//! A run advances one message at a time, outside the workspace: a consumer loads
//! the run from a [`RunStore`], advances it with the engine, and sends the next
//! message on a [`WorkBus`]. Both are traits, so Cloudflare (Queues and a
//! Durable Object per run) and other hosts (SQS, NATS, Postgres) can provide them.
//!
//! Delivery is at least once and may be out of order. Correctness comes from the
//! store's compare-and-swap on a version, not from the bus:
//!
//! - every message names the run version it was sent for, and a consumer acts
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
use crate::{ApiError, ExternalTask, Mode, RunRecord, error};
use connector_sdk::{Outcome, Task};
use serde::{Deserialize, Serialize};
pub use workflow_engine::RunStatus;

/// "This run has work", for the version it was sent for. Messages carry no data:
/// consumers always re-read the run.
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
    /// The run changed since it was loaded: another consumer owns this step.
    Conflict,
    Failed(String),
}

/// Durable run state with compare-and-swap on a version.
pub trait RunStore {
    /// The run and its version, or `None` if it doesn't exist.
    fn load(&self, run: &str) -> Result<Option<(RunRecord, u64)>, ApiError>;
    /// Save if the stored version is still `expected` (`None` to create), and
    /// return the new version.
    fn save(&self, run: &str, record: &RunRecord, expected: Option<u64>) -> Result<u64, SaveError>;
}

/// What a consumer must do after [`begin`].
#[derive(Debug, PartialEq)]
pub enum Step {
    /// Nothing to do: the message was stale or a duplicate, the run is waiting,
    /// or it has finished. Acknowledge the message.
    Done(Progress),
    /// Perform this task's network I/O (with its idempotency key), then call
    /// [`finish`] with the outcome.
    Perform { run: String, task: Task },
}

/// Status changes for the workspace's run index.
#[derive(Debug, PartialEq)]
pub enum Progress {
    /// Nothing changed that the index tracks.
    Unchanged,
    /// The run was saved at this version, with this status.
    Saved { version: u64, status: RunStatus },
}

/// Whether a workflow can run on queues yet. Children, Each, data operations and
/// webhook response windows still run in the workspace.
pub fn eligible(graph: &workflow_schema::Workflow) -> bool {
    !graph.nodes.iter().any(|n| {
        matches!(
            n.kind.as_str(),
            "Execute Workflow" | "Each / Loop" | "Respond to Webhook"
        ) || connectors_data::handles(&n.kind)
    })
}

/// Create a run's first state in the store and send its first message.
pub fn start(
    store: &dyn RunStore,
    bus: &dyn WorkBus,
    mut record: RunRecord,
    now: u64,
) -> Result<Progress, ApiError> {
    if record.parent.is_some() || !record.children.is_empty() {
        return Err(error(400, "Invoked runs still run in the workspace"));
    }
    record.advance(now)?;
    let run = record.id_string()?;
    let version = match store.save(&run, &record, None) {
        Ok(version) => version,
        Err(SaveError::Conflict) => return Err(error(409, "Run already exists")),
        Err(SaveError::Failed(message)) => return Err(error(500, message)),
    };
    schedule(bus, &record, &run, version, now)?;
    Ok(Progress::Saved {
        version,
        status: record.status()?,
    })
}

/// Process one message: advance the run and save it, or claim a task for the host
/// to perform.
pub fn begin(
    store: &dyn RunStore,
    bus: &dyn WorkBus,
    message: &WorkMessage,
    now: u64,
) -> Result<Step, ApiError> {
    let Some((mut record, version)) = store.load(&message.run)? else {
        return Ok(Step::Done(Progress::Unchanged));
    };
    // Only the message for the current version may act. Older ones were
    // superseded by a later save, which sent its own message.
    if version != message.version || !active(&record)? {
        return Ok(Step::Done(Progress::Unchanged));
    }
    if record.wake_at.is_some_and(|at| at > now) && !claimable(&record, now) {
        // Delivered early (or a duplicate of a delayed message): wait again.
        schedule(bus, &record, &message.run, version, now)?;
        return Ok(Step::Done(Progress::Unchanged));
    }
    record.advance(now)?;
    if let Some(task) = claim(&mut record, now) {
        // Persist the claim before any network I/O: if this fails, another
        // consumer owns the step.
        return match store.save(&message.run, &record, Some(version)) {
            Ok(claimed) => {
                // If the host never reports back, the task's deadline settles it
                // as "outcome unknown", as in the workspace. `finish` supersedes
                // this message by saving a newer version.
                bus.send(
                    WorkMessage {
                        run: message.run.clone(),
                        version: claimed,
                    },
                    Some(task.deadline_ms.saturating_sub(now)),
                )?;
                Ok(Step::Perform {
                    run: message.run.clone(),
                    task,
                })
            }
            Err(SaveError::Conflict) => Ok(Step::Done(Progress::Unchanged)),
            Err(SaveError::Failed(message)) => Err(error(500, message)),
        };
    }
    save_and_schedule(store, bus, &message.run, &record, version, now)
}

/// Apply a performed task's outcome, advance the run, save it and send its next
/// message. A stale outcome (the run moved on, or the task was never claimed) is
/// dropped.
pub fn finish(
    store: &dyn RunStore,
    bus: &dyn WorkBus,
    run: &str,
    task_id: &str,
    outcome: Outcome,
    now: u64,
) -> Result<Progress, ApiError> {
    let Some((mut record, version)) = store.load(run)? else {
        return Ok(Progress::Unchanged);
    };
    if !record.complete_claimed(task_id, outcome, now)? {
        return Ok(Progress::Unchanged);
    }
    match save_and_schedule(store, bus, run, &record, version, now)? {
        Step::Done(progress) => Ok(progress),
        Step::Perform { .. } => unreachable!("save_and_schedule never claims"),
    }
}

fn active(record: &RunRecord) -> Result<bool, ApiError> {
    Ok(matches!(record.status()?, RunStatus::Running))
}
fn claimable(record: &RunRecord, now: u64) -> bool {
    record
        .external
        .iter()
        .chain(record.pending.iter())
        .any(|e| !e.dispatched && now < e.task.deadline_ms)
}
/// Mark the first undispatched HTTP task as dispatched and return it.
fn claim(record: &mut RunRecord, now: u64) -> Option<Task> {
    let external: &mut ExternalTask = record
        .external
        .iter_mut()
        .chain(record.pending.iter_mut())
        .find(|e| !e.dispatched && now < e.task.deadline_ms)?;
    external.dispatched = true;
    let task = external.task.clone();
    // The run next needs the engine when a claimed task's deadline passes (or
    // sooner, when `finish` reports an outcome).
    record.wake_at = record
        .external
        .iter()
        .chain(record.pending.iter())
        .filter(|e| e.dispatched)
        .map(|e| e.task.deadline_ms)
        .min();
    Some(task)
}
fn save_and_schedule(
    store: &dyn RunStore,
    bus: &dyn WorkBus,
    run: &str,
    record: &RunRecord,
    version: u64,
    now: u64,
) -> Result<Step, ApiError> {
    let version = match store.save(run, record, Some(version)) {
        Ok(version) => version,
        Err(SaveError::Conflict) => return Ok(Step::Done(Progress::Unchanged)),
        Err(SaveError::Failed(message)) => return Err(error(500, message)),
    };
    schedule(bus, record, run, version, now)?;
    Ok(Step::Done(Progress::Saved {
        version,
        status: record.status()?,
    }))
}
/// Send the run's next message for `version`, if it has more work: now, or when
/// it next needs the engine.
fn schedule(
    bus: &dyn WorkBus,
    record: &RunRecord,
    run: &str,
    version: u64,
    now: u64,
) -> Result<(), ApiError> {
    if !active(record)? {
        return Ok(());
    }
    let delay = if claimable(record, now) {
        None
    } else {
        match record.wake_at {
            Some(at) if at > now => Some(at - now),
            Some(_) => None,
            // Running with nothing scheduled: advance again now.
            None => None,
        }
    };
    bus.send(
        WorkMessage {
            run: run.into(),
            version,
        },
        delay,
    )
}

impl RunRecord {
    /// Apply the outcome of a claimed task, then advance. Returns `false` if the
    /// task isn't one this run claimed and is still waiting on.
    pub(crate) fn complete_claimed(
        &mut self,
        task_id: &str,
        outcome: Outcome,
        now: u64,
    ) -> Result<bool, ApiError> {
        if !self
            .external
            .iter()
            .chain(self.pending.iter())
            .any(|e| e.dispatched && e.task.id == task_id)
        {
            return Ok(false);
        }
        let now = if self.mode == Mode::Simulation {
            now.max(self.logical_clock)
        } else {
            now
        };
        let mut run = self.restore()?;
        if !matches!(run.status(), RunStatus::Running | RunStatus::Paused) {
            return Ok(false);
        }
        run.complete(task_id, outcome, now)
            .map_err(|e| error(500, e.to_string()))?;
        if self.external.as_ref().is_some_and(|e| e.task.id == task_id) {
            self.external = None;
        }
        self.pending.retain(|e| e.task.id != task_id);
        self.store(&run, 500)?;
        self.advance(now)?;
        Ok(true)
    }
}

/// Pause, resume or cancel a stored run, with compare-and-swap (retried if a step
/// saves in between), then schedule its next message. Returns the run as saved.
pub fn control(
    store: &dyn RunStore,
    bus: &dyn WorkBus,
    run: &str,
    action: &str,
    now: u64,
) -> Result<(RunRecord, Progress), ApiError> {
    for _ in 0..8 {
        let Some((mut record, version)) = store.load(run)? else {
            return Err(error(404, "Run not found"));
        };
        record.control(action, now)?;
        match store.save(run, &record, Some(version)) {
            Ok(saved) => {
                schedule(bus, &record, run, saved, now)?;
                let status = record.status()?;
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
