//! The Rust application layer for the Cloudflare Worker.
//! All workspace data is persistent. No sample workflows, seed data or expiry mode.
mod audit;
pub mod contracts;
pub mod definition;
pub mod execution;
mod http_api;
pub mod invocation;
mod items;
mod publication;
pub mod store;
mod webhook_response;
use chrono::{DateTime, SecondsFormat, Utc};
use chrono_tz::Tz;
use connector_sdk::{Connector, FailureCode, Outcome, Task};
use connectors_core::CoreConnector;
use croner::Cron;
pub use publication::{Lifecycle, Revision};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::str::FromStr;
use store::RunQueries;
pub use webhook_response::{Reply as WebhookReply, ResponseWindow};
/// Read a stored run checkpoint as JSON (checkpoints may be stored compactly).
pub use workflow_engine::checkpoint_state;
use workflow_engine::{Dispatch, Limits, NodeState, Run, RunStatus};
use workflow_schema::Workflow;

/// Where a workspace reads the stored runs it doesn't hold in memory.
#[derive(Clone, Default)]
pub struct Archive(Option<std::rc::Rc<dyn store::RunArchive>>);
impl Archive {
    pub fn new(archive: std::rc::Rc<dyn store::RunArchive>) -> Self {
        Self(Some(archive))
    }
}
impl std::fmt::Debug for Archive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "Archive(attached)"
        } else {
            "Archive(none)"
        })
    }
}

/// The encoded size of what a workspace holds in memory: workflows, revisions,
/// active runs (and finished runs something still depends on) and audit entries.
/// Finished runs are archived to storage and don't count.
///
/// It protects the Durable Object's memory (128 MB per isolate), not storage.
/// Measured on workerd, a request's memory grows about ten times this state
/// (decoded copies, the committed copy and per-entity encodings), so 6 MB keeps
/// a full workspace well inside the limit.
pub const MAX_STATE_BYTES: usize = 6_000_000;
/// New runs start only while in-memory state is below this share of
/// `MAX_STATE_BYTES`, leaving the rest for active runs to finish.
pub const ADMISSION_STATE_BYTES: usize = MAX_STATE_BYTES / 4 * 3;
/// Parses the host's `MAX_ACTIVE_RUNS` setting: empty, `0` or `unlimited` mean no
/// count limit; otherwise a positive integer. Anything else is a configuration error.
pub fn parse_max_active_runs(value: &str) -> Result<Option<usize>, String> {
    match value.trim() {
        "" | "0" | "unlimited" => Ok(None),
        text => text
            .parse::<usize>()
            .map(Some)
            .map_err(|_| "MAX_ACTIVE_RUNS must be a positive integer, 0 or unlimited".into()),
    }
}
/// Encoded JSON size without building the string.
fn encoded_len(value: &impl Serialize) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    serde_json::to_writer(&mut count, value).map_or(usize::MAX, |_| count.0)
}
pub const MAX_BODY_BYTES: usize = 131_072;
#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
    pub diagnostics: Vec<workflow_schema::Diagnostic>,
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for ApiError {}
fn error(status: u16, message: impl Into<String>) -> ApiError {
    ApiError {
        status,
        message: message.into(),
        diagnostics: Vec::new(),
    }
}
fn timestamp(now: u64) -> Result<DateTime<Utc>, ApiError> {
    DateTime::from_timestamp_millis(now as i64).ok_or_else(|| error(400, "Invalid timestamp"))
}
fn iso(now: u64) -> String {
    timestamp(now)
        .map(|v| v.to_rfc3339_opts(SecondsFormat::Millis, true))
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Publication {
    Draft,
    Active,
    Paused,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_from: Option<String>,
    #[serde(flatten)]
    pub graph: Workflow,
    pub status: Publication,
    #[serde(default)]
    pub lifecycle: Option<Lifecycle>,
    #[serde(default)]
    pub has_unpublished_changes: bool,
    #[serde(default)]
    pub archived_at: Option<String>,
    pub updated_at: String,
    /// Optimistic concurrency version, incremented by every saved mutation.
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub next_scheduled_at: Option<u64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Run,
    Simulation,
    Schedule,
    Webhook,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalTask {
    pub task: Task,
    pub dispatched: bool,
}
impl ExternalTask {
    /// Whether the host may claim this task now: an HTTP task (invocations and
    /// data operations are settled by the workspace) not yet dispatched and not
    /// past its deadline.
    pub(crate) fn performable(&self, now: u64) -> bool {
        !matches!(
            self.task.node.kind.as_str(),
            "Execute Workflow" | "Each / Loop"
        ) && !connectors_data::handles(&self.task.node.kind)
            && !self.dispatched
            && now < self.task.deadline_ms
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    #[serde(default)]
    pub invocation_targets: std::collections::BTreeMap<String, Workflow>,
    #[serde(default)]
    pub parent: Option<invocation::Parent>,
    #[serde(default)]
    pub children: Vec<invocation::Child>,
    #[serde(default)]
    pub iterations: std::collections::BTreeMap<String, items::Iteration>,
    #[serde(default)]
    pub child_count: u32,
    #[serde(default)]
    pub external: Option<ExternalTask>,
    #[serde(default)]
    pub pending: Vec<ExternalTask>,
    #[serde(default)]
    pub logical_clock: u64,
    #[serde(default)]
    pub trigger_context: Value,
    #[serde(default)]
    pub webhook_response: Option<ResponseWindow>,
    pub checkpoint: String,
    pub mode: Mode,
    pub created_at: u64,
    pub wake_at: Option<u64>,
    pub host_error: Option<String>,
    /// The run's ID and engine status, cached from the checkpoint by `store` so
    /// lookups and retention do not deserialize every run. Records stored before
    /// these fields existed are backfilled on load.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_status: Option<RunStatus>,
    /// The engine-derived part of the run's list entry (see [`run_summary`]),
    /// cached by `store` so listing and indexing a run never restores its
    /// checkpoint. Absent on records stored before it existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<Value>,
}

/// The parts of a run's list entry that come from its engine state: workflow,
/// node statuses (without outputs) and failure.
fn run_summary(run: &Run) -> Value {
    json!({
        "id": run.id(),
        "workflowId": run.workflow().id,
        "name": run.workflow().name,
        "revision": run.workflow().revision,
        "nodes": node_views(run, true),
        "failure": run.failure(),
    })
}
/// Node entries for a run's view; with `summary`, without outputs.
fn node_views(run: &Run, summary: bool) -> Vec<Value> {
    run.workflow().nodes.iter().map(|n| {
        let s=&run.nodes()[&n.id];
        let (status,attempts,output,code,until)=match s {
            NodeState::Pending=>("pending",0,None,None,None),
            NodeState::Running{task}=>("running",task.attempt,None,None,Some(task.deadline_ms)),
            NodeState::Waiting{until_ms,output,attempts}=>("waiting",*attempts,Some(output),None,Some(*until_ms)),
            NodeState::Retry{at_ms,attempts}=>("retry",*attempts,None,None,Some(*at_ms)),
            NodeState::Succeeded{output,attempts,..}=>("succeeded",*attempts,Some(output),None,None),
            NodeState::Skipped=>("skipped",0,None,None,None),
            NodeState::Failed{failure,attempts}=>("failed",*attempts,None,Some(&failure.code),None),
        };
        let mut value=json!({"id":n.id,"label":n.label,"type":n.kind,"status":status,"attempts":attempts,"untilMs":until,"error":code});
        if !summary && let Some(output)=output { value["output"]=output.clone(); }
        value
    }).collect()
}
impl RunRecord {
    fn restore(&self) -> Result<Run, ApiError> {
        Run::restore(&self.checkpoint).map_err(|_| error(500, "Invalid stored run checkpoint"))
    }
    /// Persist the engine state and refresh the cached ID and status together.
    fn store(&mut self, run: &Run, status: u16) -> Result<(), ApiError> {
        self.checkpoint = run.checkpoint().map_err(|e| error(status, e.to_string()))?;
        self.summary = Some(run_summary(run));
        self.run_id = run.id().into();
        self.run_status = Some(run.status());
        Ok(())
    }
    fn backfill(&mut self) {
        if self.run_id.is_empty() || self.run_status.is_none() {
            if let Ok(run) = self.restore() {
                self.run_id = run.id().into();
                self.run_status = Some(run.status());
            }
        }
    }
    /// Whether this record holds the run with `id`, without restoring it.
    /// Pause, resume or cancel the run. Hosts that keep runs outside the
    /// workspace apply control requests with this too.
    pub fn control(&mut self, action: &str, now: u64) -> Result<(), ApiError> {
        let mut run = self.restore()?;
        match action {
            "pause" => run.pause(),
            "resume" => run.resume(),
            "cancel" => run.cancel(),
            _ => return Err(error(404, "Unknown run action")),
        }
        .map_err(|e| error(409, e.to_string()))?;
        self.store(&run, 400)?;
        self.wake_at = None;
        if action == "cancel" {
            self.external = None;
            self.pending.clear();
        }
        if action == "resume" {
            self.advance(now)?
        }
        self.settle_response(now)
    }
    pub fn matches_id(&self, id: &str) -> bool {
        if self.run_id.is_empty() {
            self.matches_id(id)
        } else {
            self.run_id == id
        }
    }
    pub fn id_string(&self) -> Result<String, ApiError> {
        if self.run_id.is_empty() {
            Ok(self.id_string()?)
        } else {
            Ok(self.run_id.clone())
        }
    }
    pub fn status(&self) -> Result<RunStatus, ApiError> {
        Ok(if self.host_error.is_some() {
            RunStatus::Failed
        } else if let Some(status) = self.run_status {
            status
        } else {
            self.restore()?.status()
        })
    }
    pub fn view(&self, summary: bool) -> Result<Value, ApiError> {
        // A list entry needs only the cached summary; a full view restores the
        // run (once) for outputs and input.
        let (engine, input) = match (&self.summary, summary) {
            (Some(cached), true) => (std::borrow::Cow::Borrowed(cached), Value::Null),
            _ => {
                let run = self.restore()?;
                let mut engine = run_summary(&run);
                engine["nodes"] = json!(node_views(&run, summary));
                let input = if summary {
                    Value::Null
                } else {
                    run.input().clone()
                };
                (std::borrow::Cow::Owned(engine), input)
            }
        };
        Ok(
            json!({"id":engine["id"],"workflowId":engine["workflowId"],"name":engine["name"],"revision":engine["revision"],"createdAt":iso(self.created_at),"mode":self.mode,"status":self.status()?,"nodes":engine["nodes"],"webhookResponse":if summary {self.webhook_response.as_ref().map(|w|json!({"delivery":w.delivery,"deadlineMs":w.deadline_ms,"closedAt":w.closed_at}))}else{self.webhook_response.as_ref().map(|w|json!(w))},"parent":self.parent,"children":self.children,"iterations":if summary {Value::Null}else{json!(self.iterations)},"context":if summary {Value::Null}else{self.trigger_context.clone()},"input":input,"error":self.host_error.clone().map(Value::String).unwrap_or_else(||engine["failure"].clone()),"wakeAt":self.wake_at}),
        )
    }
    fn advance(&mut self, now: u64) -> Result<(), ApiError> {
        if self.mode != Mode::Simulation {
            self.logical_clock = now;
        }
        self.settle_response(now)?;
        let mut run = self.restore()?;
        let parallel = workflow_engine::concurrency(run.workflow()) > 1;
        if parallel {
            if let Some(legacy) = self.external.take() {
                self.pending.push(legacy)
            }
            if run.status() == RunStatus::Paused {
                self.wake_at = None;
                return Ok(());
            }
        }
        if let Some(external) = &self.external {
            if run.status() == RunStatus::Paused {
                self.wake_at = None;
                return Ok(());
            }
            if connectors_data::handles(&external.task.node.kind)
                || matches!(
                    external.task.node.kind.as_str(),
                    "Execute Workflow" | "Each / Loop"
                )
            {
                self.wake_at = Some(if external.task.node.kind == "Each / Loop" {
                    external.task.deadline_ms
                } else {
                    now.saturating_add(1)
                });
                return Ok(());
            }
            if now < external.task.deadline_ms {
                self.wake_at = Some(if external.dispatched {
                    external.task.deadline_ms
                } else {
                    now.saturating_add(1)
                });
                return Ok(());
            }
            run.complete(
                &external.task.id,
                connectors_http::failure(FailureCode::OutcomeUnknown, false),
                now,
            )
            .map_err(|e| error(500, e.to_string()))?;
            self.external = None;
        }
        let mut clock = if self.mode == Mode::Simulation {
            self.logical_clock.max(self.created_at)
        } else {
            now
        };
        self.wake_at = None;
        let budget = if self.mode == Mode::Simulation {
            1000
        } else {
            20
        };
        for _ in 0..budget {
            let dispatch = if parallel {
                run.next_parallel(clock)
            } else {
                run.next(clock)
            }
            .map_err(|e| error(400, e.to_string()))?;
            self.pending.retain(|e| {
                run.nodes()
                    .values()
                    .any(|s| matches!(s,NodeState::Running{task} if task.id==e.task.id))
            });
            match dispatch {
                Dispatch::Task(task) => {
                    if matches!(task.node.kind.as_str(), "Execute Workflow" | "Each / Loop")
                        || connectors_data::handles(&task.node.kind)
                        || (matches!(
                            task.node.kind.as_str(),
                            "HTTP Request" | connectors_clairic::KIND | connectors_email::SEND
                        ) && self.mode != Mode::Simulation)
                    {
                        let external = ExternalTask {
                            task: *task,
                            dispatched: false,
                        };
                        if parallel {
                            self.pending.push(external);
                        } else {
                            self.external = Some(external);
                        }
                        self.wake_at = Some(now.saturating_add(1));
                        if parallel {
                            continue;
                        } else {
                            break;
                        }
                    }
                    let outcome = if task.node.kind == "Respond to Webhook" {
                        match connectors_webhook::response::prepare(&task.node.config, &task.input)
                        {
                            Ok(response) => {
                                let window = self
                                    .webhook_response
                                    .get_or_insert_with(|| ResponseWindow::new(false, clock));
                                if window.prepare(&task.node.id, response, clock) {
                                    Outcome::success(task.input.clone())
                                } else {
                                    connectors_http::failure(FailureCode::DuplicateResponse, false)
                                }
                            }
                            Err(code) => connectors_http::failure(
                                match code {
                                    connectors_webhook::response::ResponseError::MissingInput => {
                                        FailureCode::MissingInput
                                    }
                                    connectors_webhook::response::ResponseError::BudgetExceeded => {
                                        FailureCode::BudgetExceeded
                                    }
                                    _ => FailureCode::InvalidConfig,
                                },
                                false,
                            ),
                        }
                    } else if task.node.kind == "Request Context" {
                        let part = task
                            .node
                            .config
                            .get("part")
                            .and_then(Value::as_str)
                            .unwrap_or("metadata");
                        Outcome::success(match part {
                            "body" => run.input().clone(),
                            "raw_body" => self
                                .trigger_context
                                .get("rawBody")
                                .cloned()
                                .unwrap_or_else(|| {
                                    if run.input().is_string() {
                                        run.input().clone()
                                    } else {
                                        json!(run.input().to_string())
                                    }
                                }),
                            "headers" => self
                                .trigger_context
                                .pointer("/request/headers")
                                .cloned()
                                .unwrap_or_else(|| json!({})),
                            "query" => self
                                .trigger_context
                                .pointer("/request/query")
                                .cloned()
                                .unwrap_or_else(|| json!({})),
                            _ => {
                                let mut metadata = self.trigger_context.clone();
                                if let Some(object) = metadata.as_object_mut() {
                                    object.remove("rawBody");
                                }
                                metadata
                            }
                        })
                    } else if task.node.kind == "HTTP Request" {
                        connectors_http::simulate(&task)
                    } else if task.node.kind == connectors_clairic::KIND {
                        connectors_clairic::simulate(&task)
                    } else if task.node.kind == connectors_email::SEND {
                        connectors_email::simulate(&task)
                    } else if task.node.kind == connectors_email::TRIGGER {
                        Outcome::success(task.input.clone())
                    } else {
                        CoreConnector.execute(&task)
                    };
                    run.complete(&task.id, outcome, clock)
                        .map_err(|e| error(400, e.to_string()))?;
                    if task.node.kind == "Respond to Webhook"
                        && self
                            .webhook_response
                            .as_ref()
                            .is_some_and(|w| w.deadline_ms.is_some())
                    {
                        self.wake_at = Some(now.saturating_add(1));
                        break;
                    }
                }
                Dispatch::SleepUntil(at) => {
                    if self.mode == Mode::Simulation && self.pending.is_empty() {
                        clock = at
                    } else {
                        self.wake_at = Some(at);
                        break;
                    }
                }
                Dispatch::Paused | Dispatch::Finished(_) => break,
            }
        }
        if run.status() == RunStatus::Running && self.wake_at.is_none() {
            self.wake_at = Some(clock.saturating_add(1));
        }
        if run.status() == RunStatus::Running
            && self.pending.iter().any(|e| {
                !e.dispatched
                    && !matches!(
                        e.task.node.kind.as_str(),
                        "Execute Workflow" | "Each / Loop"
                    )
            })
        {
            self.wake_at = Some(self.wake_at.unwrap_or(u64::MAX).min(now.saturating_add(1)));
        }
        if matches!(
            run.status(),
            RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
        ) {
            self.pending.clear();
            self.external = None;
        }
        self.logical_clock = clock;
        self.store(&run, 400)?;
        self.settle_response(clock)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookRegistration {
    pub workflow_id: String,
    pub endpoint_id: String,
    pub token_hash: String,
}
/// What a webhook delivery needs to authenticate, derived from the live published
/// trigger. Hosts persist these next to the workspace records so requests with bad
/// credentials can be rejected without loading the whole workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookAuth {
    pub workflow_id: String,
    pub endpoint_id: String,
    pub token_hash: String,
    /// `active`, `inactive` (not published or no Webhook trigger) or `missing`
    /// (no live document), matching the full delivery path's 404 responses.
    pub state: String,
    pub authentication: String,
    pub query_parameter: String,
    pub signing_secret: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub endpoint_id: String,
    pub event_id: String,
    pub fingerprint: String,
    pub run_id: String,
    pub received_at: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    /// Host-configured cap on concurrently active runs; `None` (the default) sets
    /// no count limit. Not persisted: hosts apply their configuration after load.
    #[serde(skip)]
    pub max_active_runs: Option<usize>,
    /// Stored runs that aren't held in `runs`: finished runs nothing in memory
    /// depends on. Attached by the host after loading; without one, the
    /// workspace's runs are all there is.
    #[serde(skip)]
    pub archive: Archive,
    /// How long finished runs stay in stored history, resolved by the host at
    /// load. Applied when the workspace is committed through a store.
    #[serde(skip)]
    pub retention: store::RunRetention,
    /// Whether eligible top-level runs execute outside the workspace (in run
    /// stores) instead of in it. Set by the host.
    #[serde(skip)]
    pub offload: bool,
    /// Runs leaving for execution outside the workspace (see
    /// [`Workspace::offload_waiting`]). The next commit rewrites them as index rows
    /// and removes them from `runs`; the host then starts them in run stores.
    #[serde(skip)]
    pub leaving: std::collections::BTreeSet<String>,
    /// Control requests (run, action) for runs executing outside the workspace,
    /// accepted in this operation. The host forwards them after the commit.
    #[serde(skip)]
    pub remote_controls: Vec<(String, String)>,
    /// Internal management credentials and receipts. Never included in API snapshots.
    #[serde(default)]
    pub management: Value,
    #[serde(default)]
    pub audit: Vec<Value>,
    #[serde(default)]
    pub audit_sequence: u64,
    #[serde(default)]
    pub webhooks: Vec<WebhookRegistration>,
    #[serde(default)]
    pub webhook_events: Vec<WebhookEvent>,
    #[serde(default)]
    pub revisions: Vec<Revision>,
    pub workflows: Vec<Document>,
    pub runs: Vec<RunRecord>,
}
impl Workspace {
    pub fn from_json(text: &str) -> Result<Self, ApiError> {
        if text.len() > MAX_STATE_BYTES {
            return Err(error(500, "Stored workspace exceeds size limit"));
        }
        let ws: Self =
            serde_json::from_str(text).map_err(|_| error(500, "Invalid stored workspace"))?;
        Ok(ws.migrated())
    }
    /// Like `from_json` for a document already parsed into a `Value`, such as one
    /// assembled from stored records. `encoded_bytes` is the stored size it came
    /// from, which is held to the same limit.
    pub fn from_value(value: Value, encoded_bytes: usize) -> Result<Self, ApiError> {
        if encoded_bytes > MAX_STATE_BYTES {
            return Err(error(500, "Stored workspace exceeds size limit"));
        }
        let ws: Self =
            serde_json::from_value(value).map_err(|_| error(500, "Invalid stored workspace"))?;
        Ok(ws.migrated())
    }
    /// Upgrades applied to every loaded workspace, whatever form it was read from.
    fn migrated(mut self) -> Self {
        self.migrate_publications();
        for record in &mut self.runs {
            record.backfill();
        }
        self
    }
    /// Whether another run (or child run) may start now. Runs are not limited by
    /// count unless the host configures `max_active_runs`. Active runs can never be
    /// pruned, so new work is refused once state that retention cannot reclaim
    /// passes `ADMISSION_STATE_BYTES`: the remaining budget is kept for active runs
    /// to record progress and finish, and the workspace can always still be saved.
    pub(crate) fn admit_run(&self, count_open_responses: bool) -> Result<(), ApiError> {
        if let Some(limit) = self.max_active_runs {
            let remote = match &self.archive.0 {
                Some(archive) => archive
                    .remote_active()
                    .map_err(|e| error(500, e.to_string()))?,
                None => 0,
            };
            if self.active_runs(count_open_responses)? + remote >= limit {
                return Err(error(
                    429,
                    "Wait for an active run to finish before starting another",
                ));
            }
        }
        // Terminal runs that retention may prune do not count: the save after a
        // start reclaims them if needed. Index 0 (newest) is never pruned.
        let reclaimable: usize = (1..self.runs.len())
            .filter(|i| self.retention_removable(*i))
            .map(|i| encoded_len(&self.runs[i]) + 1)
            .sum();
        if encoded_len(self).saturating_sub(reclaimable) > ADMISSION_STATE_BYTES {
            return Err(error(
                429,
                "Too much active run state in this workspace; wait for active runs to finish before starting another",
            ));
        }
        Ok(())
    }
    /// Apply retention and encode the whole workspace. Hosts that store entities
    /// separately use [`Workspace::retain`] with sizes they already have instead.
    pub fn checkpoint(&mut self) -> Result<String, ApiError> {
        let text =
            serde_json::to_string(self).map_err(|_| error(500, "Cannot encode workspace"))?;
        if text.len() <= MAX_STATE_BYTES && self.runs.len() <= 100 {
            return Ok(text);
        }
        let runs: Vec<usize> = self.runs.iter().map(|r| encoded_len(r) + 1).collect();
        let audit: Vec<usize> = self.audit.iter().map(|e| encoded_len(e) + 1).collect();
        self.retain(text.len(), &runs, &audit)?;
        serde_json::to_string(self).map_err(|_| error(500, "Cannot encode workspace"))
    }
    /// Retention, from encoded sizes rather than by encoding the workspace:
    /// `total` is the workspace's encoded size, and `run_sizes` and `audit_sizes`
    /// the encoded size of each run and audit entry, in order, each including its
    /// separator. Keeps at most 100 runs and keeps the workspace within
    /// `MAX_STATE_BYTES` by removing the oldest removable runs, then (if that isn't
    /// enough) the oldest audit entries. The newest run and audit entry are kept.
    pub fn retain(
        &mut self,
        total: usize,
        run_sizes: &[usize],
        audit_sizes: &[usize],
    ) -> Result<(), ApiError> {
        if run_sizes.len() != self.runs.len() || audit_sizes.len() != self.audit.len() {
            return Err(error(500, "Retention sizes do not match the workspace"));
        }
        let mut excess = total.saturating_sub(MAX_STATE_BYTES);
        let mut surplus = self.runs.len().saturating_sub(100);
        let mut index = self.runs.len();
        while index > 1 && (excess > 0 || surplus > 0) {
            index -= 1;
            if self.retention_removable(index) {
                self.runs.remove(index);
                excess = excess.saturating_sub(run_sizes[index]);
                surplus = surplus.saturating_sub(1);
            }
        }
        if excess > 0 && self.audit.len() > 1 {
            // Audit retention is bounded by both count and available storage.
            let mut drop = 0;
            while drop < self.audit.len() - 1 && excess > 0 {
                excess = excess.saturating_sub(audit_sizes[drop]);
                drop += 1;
            }
            self.audit.drain(..drop);
        }
        if excess > 0 {
            return Err(error(
                413,
                "Workspace checkpoint budget exceeded; reduce stored inputs/outputs",
            ));
        }
        Ok(())
    }
    /// Whether the run at `index` can leave memory: it has finished, no active run
    /// depends on it, and it holds no open webhook response.
    pub fn run_archivable(&self, index: usize) -> bool {
        self.retention_removable(index)
    }
    /// A run by ID, from memory or, if it has been archived, from the store.
    pub fn find_run(&self, id: &str) -> Result<Option<std::borrow::Cow<'_, RunRecord>>, ApiError> {
        if let Some(record) = self.runs.iter().find(|r| r.matches_id(id)) {
            return Ok(Some(std::borrow::Cow::Borrowed(record)));
        }
        match &self.archive.0 {
            Some(archive) => Ok(archive
                .stored_run(id)
                .map_err(|e| error(500, e.to_string()))?
                .map(std::borrow::Cow::Owned)),
            None => Ok(None),
        }
    }
    /// List entries for the newest `limit` runs, archived ones included, newest
    /// first, optionally of one workflow and status.
    pub fn list_runs(
        &self,
        limit: usize,
        workflow_id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<Value>, ApiError> {
        if let Some(archive) = &self.archive.0 {
            return archive
                .list_runs(limit, workflow_id, status)
                .map_err(|e| error(500, e.to_string()));
        }
        let mut items = Vec::new();
        for record in &self.runs {
            if items.len() >= limit {
                break;
            }
            let view = record.view(true)?;
            if workflow_id.is_some_and(|w| view["workflowId"] != w)
                || status.is_some_and(|s| view["status"] != s)
            {
                continue;
            }
            items.push(view);
        }
        Ok(items)
    }
    /// A terminal run that no active parent or awaiting caller depends on.
    fn retention_removable(&self, index: usize) -> bool {
        !self.invocation_protected(index)
            && !self.runs[index]
                .webhook_response
                .as_ref()
                .is_some_and(|w| w.open())
            && self.runs[index].status().is_ok_and(|s| {
                matches!(
                    s,
                    RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
                )
            })
    }
    pub fn next_alarm(&self) -> Option<u64> {
        // The in-memory run queries cannot fail.
        let runs = self.next_run_wake().unwrap_or(None);
        self.workflows
            .iter()
            .filter_map(|w| w.next_scheduled_at)
            .chain(runs)
            .min()
    }
    pub fn snapshot(&self) -> Result<Value, ApiError> {
        Ok(
            json!({"apiVersion":2,"editPreconditionsRequired":true,"workflows":self.workflows.iter().filter(|w|w.archived_at.is_none()).collect::<Vec<_>>(),"runs":self.list_runs(100,None,None)?,"connectors":manifests()}),
        )
    }
    fn index(&self, id: &str) -> Result<usize, ApiError> {
        self.workflows
            .iter()
            .position(|w| w.graph.id == id)
            .ok_or_else(|| error(404, "Workflow not found"))
    }
    pub fn executable(document: &Document) -> Result<(), ApiError> {
        let diagnostics =
            definition::schema_errors(&serde_json::to_value(&document.graph).unwrap());
        if !diagnostics.is_empty() {
            let mut failure = error(400, "Invalid workflow definition");
            failure.diagnostics = diagnostics;
            return Err(failure);
        }
        let diagnostics = document.graph.diagnostics();
        if !diagnostics.is_empty() {
            let mut failure = error(400, diagnostics[0].message.clone());
            failure.diagnostics = diagnostics;
            return Err(failure);
        }
        if document
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == "Respond to Webhook")
            && !document.graph.nodes.iter().any(|n| n.kind == "Webhook")
        {
            return Err(error(400, "Respond to Webhook requires a Webhook trigger"));
        }
        let manifests = manifests();
        for (node_index, node) in document.graph.nodes.iter().enumerate() {
            let result = (|| -> Result<(), ApiError> {
                if !node.input_bindings.is_empty() {
                    let fail = |message: &str| {
                        let mut failure = error(400, message);
                        failure.diagnostics.push(
                            workflow_schema::Diagnostic::new(
                                "INVALID_INPUT_BINDINGS",
                                format!("/nodes/{node_index}/inputBindings"),
                                message,
                            )
                            .node(&node.id, &node.kind),
                        );
                        failure
                    };
                    if node.is_trigger() {
                        return Err(fail(
                            "Assemble input on a downstream node; triggers expose the original event",
                        ));
                    }
                    let config =
                        serde_json::from_value(json!({"fields":node.input_bindings})).unwrap();
                    let refs = connectors_core::expressions::validate(&config)
                        .map_err(|_| fail("Invalid or over-budget input binding expressions"))?;
                    let mut ancestors = std::collections::BTreeSet::new();
                    let mut pending = vec![node.id.clone()];
                    while let Some(id) = pending.pop() {
                        for edge in document.graph.edges.iter().filter(|e| e.target == id) {
                            if ancestors.insert(edge.source.clone()) {
                                pending.push(edge.source.clone());
                            }
                        }
                    }
                    if refs.iter().any(|id| !ancestors.contains(id)) {
                        return Err(fail("Input bindings may reference graph ancestors only"));
                    }
                }
                if node.kind == "Map / Transform" {
                    let mapping = node.config.get("mapping").cloned().unwrap_or(Value::Null);
                    let mapping = if let Value::String(text) = mapping {
                        serde_json::from_str::<Value>(&text).unwrap_or(Value::Null)
                    } else {
                        mapping
                    };
                    if !mapping.is_object() {
                        return Err(error(
                            400,
                            "Mapping must be an object or a JSON-encoded object",
                        ));
                    }
                }
                if node.kind == "Expressions" {
                    let refs = connectors_core::expressions::validate(&node.config)
                        .map_err(|_| error(400, "Invalid or over-budget expression tree"))?;
                    let mut ancestors = std::collections::BTreeSet::new();
                    let mut pending = vec![node.id.clone()];
                    while let Some(id) = pending.pop() {
                        for edge in document.graph.edges.iter().filter(|e| e.target == id) {
                            if ancestors.insert(edge.source.clone()) {
                                pending.push(edge.source.clone());
                            }
                        }
                    }
                    if refs.iter().any(|id| !ancestors.contains(id)) {
                        return Err(error(400, "Expressions can reference upstream nodes only"));
                    }
                }
                if node.kind == "Switch / Router" {
                    connectors_core::routing::validate_switch(&node.config)
                        .map_err(|_| error(400, "Invalid Switch routes or conditions"))?;
                }
                if node.kind == "Merge" {
                    connectors_core::routing::validate_merge(&node.config)
                        .map_err(|_| error(400, "Invalid Merge policy"))?;
                    if node
                        .config
                        .get("sources")
                        .and_then(Value::as_array)
                        .is_some_and(|sources| {
                            sources.iter().any(|id| {
                                !document.graph.edges.iter().any(|e| {
                                    e.target == node.id && Some(e.source.as_str()) == id.as_str()
                                })
                            })
                        })
                    {
                        return Err(error(400, "Merge sources must be connected input nodes"));
                    }
                }
                if node.kind == "Validate Schema" {
                    connectors_core::validation::validate(&node.config)
                        .map_err(|_| error(400, "Invalid or unsupported validation schema"))?;
                }
                if node.kind == "Split / Parallel" {
                    if node.config.keys().any(|k| k != "concurrency")
                        || node.config.get("concurrency").is_some_and(|v| {
                            v.as_u64()
                                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                                .is_none_or(|v| !matches!(v, 2 | 4 | 8))
                        })
                        || document
                            .graph
                            .edges
                            .iter()
                            .filter(|e| e.source == node.id && e.source_port == "success")
                            .count()
                            < 2
                    {
                        return Err(error(
                            400,
                            "Split requires at least two outgoing branches and concurrency 2, 4 or 8",
                        ));
                    }
                }
                if node.kind == "Join" {
                    let parents: Vec<_> = document
                        .graph
                        .edges
                        .iter()
                        .filter(|e| e.target == node.id)
                        .collect();
                    let distinct: std::collections::BTreeSet<_> =
                        parents.iter().map(|e| &e.source).collect();
                    if parents.len() < 2
                        || distinct.len() != parents.len()
                        || node.config.keys().any(|k| k != "policy")
                        || node
                            .config
                            .get("policy")
                            .is_some_and(|v| !matches!(v.as_str(), Some("all" | "collect")))
                    {
                        return Err(error(
                            400,
                            "Join requires distinct branch inputs and an all or collect policy",
                        ));
                    }
                }
                if matches!(
                    node.kind.as_str(),
                    "Execute Workflow" | "Execute Workflow Trigger" | "Each / Loop"
                ) {
                    invocation::validate_node(node, &document.graph)?;
                }
                if matches!(node.kind.as_str(), "Split / Batch" | "Aggregate") {
                    connectors_core::collections::validate(&node.kind, &node.config)
                        .map_err(|_| error(400, "Invalid collection configuration"))?;
                }
                if node.kind == "Parse Body" {
                    connectors_core::parsing::validate(&node.config)
                        .map_err(|_| error(400, "Invalid body parser configuration"))?;
                }
                if node.kind == "Filter" {
                    connectors_core::conditions::validate_filter(&node.config)
                        .map_err(|_| error(400, "Invalid Filter conditions"))?;
                }
                if node.kind == "IF / Condition" {
                    connectors_core::conditions::validate(&node.config)
                        .map_err(|_| error(400, "Invalid IF conditions"))?;
                }
                if node.kind == "Respond to Webhook" {
                    connectors_webhook::response::validate(&node.config)
                        .map_err(|_| error(400, "Invalid webhook response configuration"))?;
                }
                if node.kind == "Request Context"
                    && (node.config.keys().any(|k| k != "part")
                        || node.config.get("part").is_some_and(|p| {
                            !matches!(
                                p.as_str(),
                                Some("metadata" | "body" | "raw_body" | "headers" | "query")
                            )
                        }))
                {
                    return Err(error(400, "Invalid Request Context part"));
                }
                if node.kind == "Webhook" {
                    connectors_webhook::context::selection(&node.config)
                        .map_err(|_| error(400, "Invalid request context field selection"))?;
                    match node
                        .config
                        .get("authentication")
                        .and_then(Value::as_str)
                        .unwrap_or("bearer")
                    {
                        "none" | "bearer" => {}
                        "query"
                            if node
                                .config
                                .get("queryParameter")
                                .and_then(Value::as_str)
                                .is_some_and(connectors_webhook::valid_parameter)
                                || !node.config.contains_key("queryParameter") => {}
                        "hmac" | "hmac_sha256"
                            if node
                                .config
                                .get("signingSecret")
                                .and_then(Value::as_str)
                                .is_some_and(|s| !s.is_empty()) => {}
                        _ => {
                            return Err(error(400, "Invalid webhook authentication configuration"));
                        }
                    }
                }
                if connectors_data::handles(&node.kind) {
                    connectors_data::access(node)
                        .map_err(|_| error(400, "Invalid data connector configuration"))?;
                }
                if node.kind == "HTTP Request" {
                    let mut node = node.clone();
                    node.config.remove("query");
                    node.config.remove("body");
                    connectors_http::validate(&Task{context:Value::Null,id:"validate".into(),idempotency_key:"validate".into(),node,input:Value::Null,attempt:1,deadline_ms:1}).map_err(|_|error(400,"Invalid HTTP request: use HTTPS, a supported auth configuration and one attempt for writes"))?;
                }
                if node.kind == connectors_email::TRIGGER && !node.config.is_empty() {
                    return Err(error(
                        400,
                        "Email Trigger routing is configured on the deployment",
                    ));
                }
                if node.kind == connectors_email::SEND {
                    connectors_email::validate_send(&node.config)
                        .map_err(|_| error(400, "Invalid Send Email configuration"))?;
                    if node.retry.max_attempts != 1 {
                        return Err(error(
                            400,
                            "Send Email requires one attempt to avoid duplicate delivery",
                        ));
                    }
                }
                if node.kind == connectors_clairic::KIND {
                    connectors_clairic::validate(&node.config).map_err(|_| {
                        error(400, "Invalid Clairic configuration: check field names, types, allowed labels and simulation answers")
                    })?;
                }
                if !manifests
                    .iter()
                    .any(|m| m.kind == node.kind && m.version == node.version)
                {
                    return Err(error(400, format!("{} is not implemented", node.kind)));
                }
                Ok(())
            })();
            result.map_err(|mut failure| {
                if failure.diagnostics.is_empty() {
                    failure.diagnostics.push(
                        workflow_schema::Diagnostic::new(
                            "INVALID_CONNECTOR_CONFIG",
                            format!("/nodes/{node_index}/config"),
                            failure.message.clone(),
                        )
                        .node(&node.id, &node.kind),
                    );
                }
                failure
            })?;
        }
        Ok(())
    }
    fn start_run(
        &mut self,
        id: &str,
        doc: &Document,
        input: Value,
        mode: Mode,
        now: u64,
    ) -> Result<Value, ApiError> {
        let trigger = match mode {
            Mode::Run => "manual",
            Mode::Simulation => "simulation",
            Mode::Schedule => "schedule",
            Mode::Webhook => "webhook",
        };
        self.start_run_context(
            id,
            doc,
            input,
            mode,
            now,
            json!({"trigger":trigger,"request":null}),
        )
    }
    fn start_run_context(
        &mut self,
        id: &str,
        doc: &Document,
        input: Value,
        mode: Mode,
        now: u64,
        trigger_context: Value,
    ) -> Result<Value, ApiError> {
        self.admit_run(true)?;
        let record = self.new_run_record(id, doc, input, mode, now, trigger_context)?;
        self.start_record(record, now)
    }
    /// Start a prepared run in this workspace (already admitted) and return its view.
    pub fn start_record(&mut self, mut record: RunRecord, now: u64) -> Result<Value, ApiError> {
        let id = record.id_string()?;
        record.advance(now)?;
        self.runs.insert(0, record);
        self.settle_invocations(now)?;
        self.runs
            .iter()
            .find(|r| r.matches_id(&id))
            .unwrap()
            .view(false)
    }
    /// Hand waiting run trees to run stores (when the host executes runs outside
    /// the workspace): top-level, eligible runs still running, with every run
    /// they invoked. They leave on the next commit, which writes them as index
    /// rows in their places in the list, and the host then starts them. A tree is
    /// left here while it has a data task to perform (the host performs those
    /// after a save, so runs that finish on them still answer with their
    /// result), an HTTP request in flight from this workspace, or a child that
    /// isn't held in memory. Returns whether any tree is leaving.
    pub fn offload_waiting(&mut self) -> Result<bool, ApiError> {
        if !self.offload {
            return Ok(false);
        }
        let mut roots = Vec::new();
        for record in &self.runs {
            if record.parent.is_none()
                && record.status()? == RunStatus::Running
                && !self.leaving.contains(&record.id_string()?)
                && execution::eligible(record)?
            {
                roots.push(record.id_string()?);
            }
        }
        let mut moved = false;
        for root in roots {
            let in_tree = |r: &RunRecord| {
                r.matches_id(&root) || r.parent.as_ref().is_some_and(|p| p.root_id == root)
            };
            let tree: Vec<&RunRecord> = self.runs.iter().filter(|r| in_tree(r)).collect();
            let busy = tree.iter().any(|r| {
                r.external.iter().chain(r.pending.iter()).any(|e| {
                    connectors_data::handles(&e.task.node.kind)
                        || (e.dispatched
                            && !matches!(
                                e.task.node.kind.as_str(),
                                "Execute Workflow" | "Each / Loop"
                            ))
                })
            });
            let complete = tree.iter().all(|r| {
                r.children
                    .iter()
                    .all(|c| tree.iter().any(|t| t.matches_id(&c.run_id)))
            });
            if busy || !complete {
                continue;
            }
            let ids = tree
                .iter()
                .map(|r| r.id_string())
                .collect::<Result<Vec<_>, _>>()?;
            self.leaving.extend(ids);
            moved = true;
        }
        Ok(moved)
    }
    /// A new run of `doc`, not yet started or admitted. Hosts that execute runs
    /// on queues admit it, then hand it to `execution::start`.
    pub fn new_run_record(
        &self,
        id: &str,
        doc: &Document,
        input: Value,
        mode: Mode,
        now: u64,
        trigger_context: Value,
    ) -> Result<RunRecord, ApiError> {
        if trigger_context.to_string().len() > 98304 {
            return Err(error(413, "Trigger context exceeds budget"));
        }
        Self::executable(doc)?;
        let invocation_targets = self.invocation_targets(&doc.graph)?;
        let run = Run::new(id, doc.graph.clone(), input, now, Limits::default())
            .map_err(|e| error(400, e.to_string()))?;
        Ok(RunRecord {
            invocation_targets,
            parent: None,
            children: Vec::new(),
            iterations: std::collections::BTreeMap::new(),
            child_count: 0,
            external: None,
            pending: Vec::new(),
            logical_clock: now,
            webhook_response: doc
                .graph
                .nodes
                .iter()
                .any(|n| n.kind == "Respond to Webhook")
                .then(|| {
                    ResponseWindow::new(
                        mode == Mode::Webhook && trigger_context["request"].is_object(),
                        now,
                    )
                }),
            trigger_context,
            checkpoint: run.checkpoint().map_err(|e| error(400, e.to_string()))?,
            mode,
            created_at: now,
            wake_at: None,
            host_error: None,
            run_id: run.id().into(),
            run_status: Some(run.status()),
            summary: Some(crate::run_summary(&run)),
        })
    }
    pub fn record_audit(
        &mut self,
        actor: &str,
        action: &str,
        target: &str,
        outcome: &str,
        now: u64,
        correlation: &str,
    ) {
        if action.starts_with("runs")
            || action == "read"
            || action == "oauth.token_issued"
            || matches!(actor, "engine" | "scheduler" | "webhook")
        {
            return;
        }
        self.audit_sequence += 1;
        self.audit.push(json!({"schemaVersion":2,"id":self.audit_sequence,"actor":actor,"action":action,"target":target,"outcome":outcome,"at":iso(now),"correlationId":correlation,"summary":action.replace(['.','_']," "),"entity":{"type":"management","id":target,"name":target}}));
        if self.audit.len() > 1000 {
            self.audit.remove(0);
        }
    }
    pub fn audit_page(&self, after: u64, limit: usize) -> Value {
        let items: Vec<_> = self
            .audit
            .iter()
            .filter(|v| audit::visible(v) && v["id"].as_u64().unwrap_or(0) > after)
            .take(limit.clamp(1, 100))
            .cloned()
            .collect();
        json!({"items":items,"nextCursor":items.last().map(|v|&v["id"]),"oldestRetainedId":self.audit.first().map(|v|&v["id"]),"totalRecorded":self.audit_sequence,"retentionLimit":1000})
    }
    pub fn handle(
        &mut self,
        method: &str,
        path: &str,
        body: &[u8],
        now: u64,
        run_id: &str,
    ) -> Result<(u16, Value), ApiError> {
        self.handle_as(method, path, body, now, run_id, "browser")
    }
    pub fn handle_as(
        &mut self,
        method: &str,
        path: &str,
        body: &[u8],
        now: u64,
        run_id: &str,
        actor: &str,
    ) -> Result<(u16, Value), ApiError> {
        let workflow_id = path
            .strip_prefix("/api/workflows/")
            .and_then(|p| p.split('/').next());
        let before =
            workflow_id.and_then(|id| self.workflows.iter().find(|w| w.graph.id == id).cloned());
        let result = self.handle_inner(method, path, body, now, run_id, actor);
        if let Some(id) = workflow_id.filter(|_| matches!(method, "POST" | "PUT" | "DELETE")) {
            let after = self.workflows.iter().find(|w| w.graph.id == id).cloned();
            let action = if method == "PUT" {
                if before.is_none() {
                    "workflow.created"
                } else {
                    "workflow.updated"
                }
            } else if path.ends_with("/publish") {
                "workflow.published"
            } else if path.ends_with("/pause") {
                "workflow.paused"
            } else if path.ends_with("/resume") {
                "workflow.resumed"
            } else if path.ends_with("/archive") {
                "workflow.archived"
            } else if path.contains("/revisions/") {
                "workflow.revision_restored"
            } else if path.ends_with("/restore") {
                "workflow.restored"
            } else {
                "workflow.change_rejected"
            };
            self.workflow_audit(
                actor,
                action,
                id,
                before.as_ref(),
                after.as_ref(),
                result.as_ref().err().map(|e| e.status),
                now,
                run_id,
            );
        }
        result
    }
    fn handle_inner(
        &mut self,
        method: &str,
        path: &str,
        body: &[u8],
        now: u64,
        run_id: &str,
        actor: &str,
    ) -> Result<(u16, Value), ApiError> {
        self.migrate_publications();
        if body.len() > MAX_BODY_BYTES {
            return Err(error(413, "Request exceeds 128 KiB"));
        }
        if method == "GET" && path == "/api/workflows/schema" {
            return Ok((200, definition::discovery()));
        }
        if method == "POST" && path == "/api/workflows/validate" {
            let value: Value =
                serde_json::from_slice(body).map_err(|_| error(400, "Invalid JSON"))?;
            return Ok((200, definition::validate(&value)));
        }
        if method == "GET" && path == "/api/audit" {
            return Ok((200, self.audit_page(0, 100)));
        }
        if method == "GET" && path == "/api/workspace" {
            return Ok((200, self.snapshot()?));
        }
        let parts: Vec<_> = path.trim_matches('/').split('/').collect();
        if method == "GET" && path == "/api/workflows/invocable" {
            let workflows: Vec<Value> = self
                .workflows
                .iter()
                .filter_map(|w| self.live_document(&w.graph.id).ok())
                .filter(|w| {
                    w.status == Publication::Active
                        && w.graph
                            .nodes
                            .iter()
                            .any(|n| n.kind == "Execute Workflow Trigger")
                })
                .map(|w| json!({"id":w.graph.id,"name":w.graph.name,"revision":w.graph.revision}))
                .collect();
            return Ok((200, json!({"workflows":workflows})));
        }
        if method == "GET" && path == "/api/workflows/trash" {
            return Ok((
                200,
                json!({"workflows":self.workflows.iter().filter(|w|w.archived_at.is_some()).collect::<Vec<_>>()}),
            ));
        }
        if method == "POST" && path == "/api/runs" {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Input {
                workflow_id: String,
                mode: Mode,
                input: Value,
            }
            let input: Input = serde_json::from_slice(body)
                .map_err(|_| error(400, "Expected workflowId, mode and input"))?;
            if matches!(input.mode, Mode::Schedule | Mode::Webhook) {
                return Err(error(
                    400,
                    "Schedule and webhook runs must originate from their registered triggers",
                ));
            }
            let doc = self.workflows[self.index(&input.workflow_id)?].clone();
            if doc.archived_at.is_some() {
                return Err(error(404, "Workflow is in trash"));
            }
            if let Some(expected) = serde_json::from_slice::<Value>(body)
                .ok()
                .and_then(|v| v["expectedVersion"].as_u64())
            {
                if doc.version != expected {
                    return Err(error(409, "Workflow changed; reload before executing"));
                }
            }
            return Ok((
                201,
                self.start_run(run_id, &doc, input.input, input.mode, now)?,
            ));
        }
        if parts.len() < 3 || parts[0] != "api" {
            return Err(error(404, "Route not found"));
        }
        let id = parts[2];
        if id.is_empty()
            || id.len() > 256
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(error(400, "Invalid resource ID"));
        }
        if parts[1] == "workflows"
            && method == "POST"
            && parts.len() == 4
            && matches!(parts[3], "archive" | "restore")
        {
            let index = self.index(id)?;
            let input: Value = serde_json::from_slice(body)
                .map_err(|_| error(400, "Expected edit precondition"))?;
            let expected = input["expectedVersion"]
                .as_u64()
                .ok_or_else(|| error(428, "expectedVersion is required"))?;
            let mut doc = self.workflows[index].clone();
            if expected != doc.version {
                return Err(error(409, "Workflow changed; reload before moving it"));
            }
            let archive = parts[3] == "archive";
            if archive == doc.archived_at.is_some() {
                return Ok((200, serde_json::to_value(doc).unwrap()));
            }
            doc.archived_at = archive.then(|| iso(now));
            if let Some(lifecycle) = &mut doc.lifecycle {
                lifecycle.enabled = false;
            }
            doc.status = if doc
                .lifecycle
                .as_ref()
                .is_some_and(|l| l.published_revision.is_some())
            {
                Publication::Paused
            } else {
                Publication::Draft
            };
            doc.next_scheduled_at = None;
            doc.updated_at = iso(now);
            doc.version = doc
                .version
                .checked_add(1)
                .ok_or_else(|| error(409, "Version exhausted"))?;
            self.workflows[index] = doc.clone();
            return Ok((200, serde_json::to_value(doc).unwrap()));
        }
        if parts[1] == "workflows"
            && self
                .index(id)
                .is_ok_and(|i| self.workflows[i].archived_at.is_some())
            && method != "GET"
        {
            return Err(error(
                409,
                "Restore the workflow from trash before editing or publishing",
            ));
        }
        if parts[1] == "workflows" && method == "PUT" && parts.len() == 3 {
            let mut doc: Document = serde_json::from_slice(body).map_err(|_| {
                let mut failure = error(400, "Invalid workflow document");
                if let Ok(value) = serde_json::from_slice::<Value>(body) {
                    failure.diagnostics = definition::shape_errors(&value);
                }
                if failure.diagnostics.is_empty() {
                    failure.diagnostics.push(workflow_schema::Diagnostic::new(
                        "INVALID_DOCUMENT_METADATA",
                        "",
                        "Use graph fields from workflows_schema_get and valid export metadata",
                    ));
                }
                failure
            })?;
            if doc.imported_from.as_ref().is_some_and(|source| {
                source.is_empty()
                    || source.len() > 160
                    || !source
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            }) || doc.graph.id != id
                || id.len() > 160
                || !workflow_schema::SUPPORTED_SCHEMA_VERSIONS.contains(&doc.graph.schema_version)
                || doc.graph.name.len() > 160
                || doc.graph.nodes.len() > 100
                || doc.graph.edges.len() > 500
            {
                return Err(error(400, "Invalid workflow metadata or graph size"));
            }
            // Missing auth on a new trigger means public ingress. On an existing
            // trigger preserve its protection; making it public must be explicit.
            for node in doc.graph.nodes.iter_mut().filter(|n| n.kind == "Webhook") {
                if !node.config.contains_key("authentication") {
                    let previous = self
                        .workflows
                        .iter()
                        .find(|w| w.graph.id == id)
                        .and_then(|w| w.graph.nodes.iter().find(|n| n.kind == "Webhook"));
                    if let Some(previous) = previous {
                        for key in ["authentication", "signingSecret", "queryParameter"] {
                            if !node.config.contains_key(key) {
                                if let Some(value) = previous.config.get(key) {
                                    node.config.insert(key.into(), value.clone());
                                }
                            }
                        }
                    } else {
                        node.config.insert("authentication".into(), json!("none"));
                    }
                }
            }
            doc.archived_at = None;
            doc.lifecycle = Some(Lifecycle::default());
            doc.has_unpublished_changes = true;
            doc.status = Publication::Draft;
            doc.updated_at = iso(now);
            doc.next_scheduled_at = None;
            if let Ok(index) = self.index(id) {
                if serde_json::from_slice::<Value>(body)
                    .ok()
                    .is_some_and(|v| v.get("version").is_some())
                    && doc.version != self.workflows[index].version
                {
                    return Err(error(409, "Workflow changed; reload before saving"));
                }
                doc.version = self.workflows[index]
                    .version
                    .checked_add(1)
                    .ok_or_else(|| error(409, "Version exhausted"))?;
                let current = &self.workflows[index];
                doc.graph.revision = current.graph.revision;
                if doc.graph == current.graph {
                    return Ok((200, serde_json::to_value(current).unwrap()));
                }
                doc.lifecycle = current.lifecycle.clone();
                doc.imported_from = current.imported_from.clone();
                doc.status = current.status;
                doc.next_scheduled_at = current.next_scheduled_at;
                doc.has_unpublished_changes = doc
                    .lifecycle
                    .as_ref()
                    .and_then(|l| l.published_revision)
                    .and_then(|live| {
                        self.revisions
                            .iter()
                            .find(|r| r.workflow_id == id && r.revision == live)
                    })
                    .is_none_or(|r| r.fingerprint != publication::fingerprint(&doc.graph));
                self.workflows[index] = doc.clone();
            } else {
                doc.graph.revision = 0;
                doc.version = 1;
                self.workflows.push(doc.clone());
            }
            return Ok((200, serde_json::to_value(doc).unwrap()));
        }
        if parts[1] == "workflows" && parts.get(3) == Some(&"revisions") {
            let index = self.index(id)?;
            if method == "GET" && parts.len() == 4 {
                let entries: Vec<_> = self
                    .revisions
                    .iter()
                    .filter(|r| r.workflow_id == id)
                    .rev()
                    .collect();
                return Ok((
                    200,
                    json!({"revisions":entries,"needsReview":self.workflows[index].lifecycle.as_ref().is_some_and(|l|l.needs_review)}),
                ));
            }
            if method == "POST" && parts.len() == 6 && parts[5] == "restore" {
                let revision: u32 = parts[4]
                    .parse()
                    .map_err(|_| error(400, "Invalid revision"))?;
                let input: Value = serde_json::from_slice(body)
                    .map_err(|_| error(400, "Expected edit precondition"))?;
                let expected = input["expectedVersion"]
                    .as_u64()
                    .ok_or_else(|| error(428, "expectedVersion is required"))?;
                let mut doc = self.workflows[index].clone();
                if expected != doc.version {
                    return Err(error(409, "Workflow changed; reload before restoring"));
                }
                let snapshot = self
                    .revisions
                    .iter()
                    .find(|r| r.workflow_id == id && r.revision == revision)
                    .ok_or_else(|| error(404, "Revision not found"))?;
                let latest = doc.graph.revision;
                doc.graph = snapshot.graph.clone();
                doc.graph.revision = latest;
                doc.has_unpublished_changes = doc
                    .lifecycle
                    .as_ref()
                    .and_then(|l| l.published_revision)
                    .and_then(|live| {
                        self.revisions
                            .iter()
                            .find(|r| r.workflow_id == id && r.revision == live)
                    })
                    .is_none_or(|r| r.fingerprint != publication::fingerprint(&doc.graph));
                doc.version = doc
                    .version
                    .checked_add(1)
                    .ok_or_else(|| error(409, "Version exhausted"))?;
                doc.updated_at = iso(now);
                self.workflows[index] = doc.clone();
                return Ok((200, serde_json::to_value(doc).unwrap()));
            }
            return Err(error(404, "Revision route not found"));
        }
        if parts[1] == "workflows" && method == "POST" && parts.len() == 4 {
            let index = self.index(id)?;
            let mut doc = self.workflows[index].clone();
            if let Some(expected) = serde_json::from_slice::<Value>(body)
                .ok()
                .and_then(|v| v["expectedVersion"].as_u64())
            {
                if doc.version != expected {
                    return Err(error(409, "Workflow changed; reload before publishing"));
                }
            }
            match parts[3] {
                "publish" => {
                    return Ok((
                        200,
                        serde_json::to_value(self.publish(index, now, actor)?).unwrap(),
                    ));
                }
                "pause" | "resume" => {
                    let live = self.live_document(id)?;
                    let enabled = parts[3] == "resume";
                    if doc.lifecycle.as_ref().is_some_and(|l| l.enabled == enabled) {
                        return Ok((200, serde_json::to_value(doc).unwrap()));
                    }
                    if enabled {
                        Self::executable(&live)?;
                    }
                    doc.lifecycle.as_mut().unwrap().enabled = enabled;
                    doc.status = if enabled {
                        Publication::Active
                    } else {
                        Publication::Paused
                    };
                    doc.next_scheduled_at = if enabled {
                        schedule(&live.graph, now)?.map(|s| s.at)
                    } else {
                        None
                    };
                }
                _ => return Err(error(404, "Unknown workflow action")),
            }
            doc.version = doc
                .version
                .checked_add(1)
                .ok_or_else(|| error(409, "Version exhausted"))?;
            doc.updated_at = iso(now);
            self.workflows[index] = doc.clone();
            return Ok((200, serde_json::to_value(doc).unwrap()));
        }
        if parts[1] == "runs" {
            let Some(index) = self.runs.iter().position(|r| r.matches_id(id)) else {
                // Archived runs have finished: they can be read, not controlled.
                let record = self
                    .find_run(id)?
                    .ok_or_else(|| error(404, "Run not found"))?;
                let remote = match &self.archive.0 {
                    Some(archive) => archive
                        .is_remote(id)
                        .map_err(|e| error(500, e.to_string()))?,
                    None => false,
                };
                let active = matches!(record.status()?, RunStatus::Running | RunStatus::Paused);
                return match (method, parts.len()) {
                    ("GET", 3) => Ok((200, record.view(false)?)),
                    ("POST", 4)
                        if remote
                            && active
                            && matches!(parts[3], "pause" | "resume" | "cancel") =>
                    {
                        // The run executes elsewhere: check the request against
                        // its last reported state, answer with the result, and
                        // let the host forward it.
                        let mut expected = record.into_owned();
                        expected.control(parts[3], now)?;
                        self.remote_controls
                            .push((id.to_owned(), parts[3].to_owned()));
                        Ok((200, expected.view(false)?))
                    }
                    ("POST", 4) if matches!(parts[3], "pause" | "resume" | "cancel") => {
                        Err(error(409, "Run has already finished"))
                    }
                    _ => Err(error(404, "Route not found")),
                };
            };
            if method == "GET" && parts.len() == 3 {
                return Ok((200, self.runs[index].view(false)?));
            }
            if method == "POST" && parts.len() == 4 {
                let id = self.runs[index].id_string()?;
                self.runs[index].control(parts[3], now)?;
                self.settle_invocations(now)?;
                return Ok((
                    200,
                    self.runs
                        .iter()
                        .find(|r| r.matches_id(&id))
                        .unwrap()
                        .view(false)?,
                ));
            }
        }
        Err(error(404, "Route not found"))
    }
    pub fn webhook_document(&self, id: &str) -> Result<&Document, ApiError> {
        let doc = &self.workflows[self.index(id)?];
        if !doc.graph.nodes.iter().any(|n| n.kind == "Webhook") {
            return Err(error(
                400,
                "Save a Webhook trigger before configuring its endpoint",
            ));
        }
        Ok(doc)
    }
    pub fn webhook_info(&self, id: &str) -> Result<Value, ApiError> {
        let doc = self.webhook_document(id)?;
        let registration = self.webhooks.iter().find(|w| w.workflow_id == id);
        let live = self.live_document(id).ok();
        let effective = live
            .as_ref()
            .filter(|d| d.graph.nodes.iter().any(|n| n.kind == "Webhook"))
            .unwrap_or(doc);
        let trigger = effective
            .graph
            .nodes
            .iter()
            .find(|n| n.kind == "Webhook")
            .unwrap();
        Ok(
            json!({"configured":registration.is_some(),"endpointId":registration.map(|r|&r.endpoint_id),"active":doc.status==Publication::Active,"authentication":trigger.config.get("authentication").and_then(Value::as_str).unwrap_or("bearer"),"queryParameter":trigger.config.get("queryParameter").and_then(Value::as_str).unwrap_or("key")}),
        )
    }
    pub fn configure_webhook(
        &mut self,
        id: &str,
        endpoint_id: String,
        token_hash: String,
    ) -> Result<Value, ApiError> {
        self.webhook_document(id)?;
        self.webhooks.retain(|w| w.workflow_id != id);
        self.webhooks.push(WebhookRegistration {
            workflow_id: id.into(),
            endpoint_id,
            token_hash,
        });
        self.webhook_info(id)
    }
    /// Authentication descriptors for every registered endpoint. Defaults match
    /// the delivery path: omitted authentication is bearer, the query parameter is
    /// `key`, and an omitted signing secret alias is empty.
    pub fn webhook_auth(&self) -> Vec<WebhookAuth> {
        self.webhooks
            .iter()
            .map(|registration| {
                let doc = self.live_document(&registration.workflow_id).ok();
                let trigger = doc
                    .as_ref()
                    .and_then(|d| d.graph.nodes.iter().find(|n| n.kind == "Webhook"));
                let state = match (&doc, trigger) {
                    (None, _) => "missing",
                    (Some(d), Some(_)) if d.status == Publication::Active => "active",
                    _ => "inactive",
                };
                let text = |key: &str, default: &str| {
                    trigger
                        .and_then(|t| t.config.get(key))
                        .and_then(Value::as_str)
                        .unwrap_or(default)
                        .to_string()
                };
                WebhookAuth {
                    workflow_id: registration.workflow_id.clone(),
                    endpoint_id: registration.endpoint_id.clone(),
                    token_hash: registration.token_hash.clone(),
                    state: state.into(),
                    authentication: text("authentication", "bearer"),
                    query_parameter: text("queryParameter", "key"),
                    signing_secret: text("signingSecret", ""),
                }
            })
            .collect()
    }
    pub fn receive_email(
        &mut self,
        workflow_id: &str,
        payload: Value,
        now: u64,
        run_id: &str,
    ) -> Result<Value, ApiError> {
        let doc = self.live_document(workflow_id)?;
        if doc.status != Publication::Active
            || !doc
                .graph
                .nodes
                .iter()
                .any(|node| node.kind == connectors_email::TRIGGER)
        {
            return Err(error(404, "Email workflow is not active"));
        }
        self.start_run_context(
            run_id,
            &doc,
            payload,
            Mode::Run,
            now,
            json!({"trigger":"email","request":null}),
        )
    }
    pub fn receive_webhook(
        &mut self,
        id: &str,
        endpoint: &str,
        payload: Value,
        now: u64,
        run_id: &str,
    ) -> Result<Value, ApiError> {
        self.receive_webhook_context(
            id,
            endpoint,
            payload,
            now,
            run_id,
            json!({"trigger":"webhook","request":null}),
        )
    }
    pub fn receive_webhook_context(
        &mut self,
        id: &str,
        endpoint: &str,
        payload: Value,
        now: u64,
        run_id: &str,
        context: Value,
    ) -> Result<Value, ApiError> {
        let doc = self.live_document(id)?;
        if !doc.graph.nodes.iter().any(|n| n.kind == "Webhook") {
            return Err(error(404, "Webhook is not active"));
        }
        if doc.status != Publication::Active
            || !self
                .webhooks
                .iter()
                .any(|w| w.workflow_id == id && w.endpoint_id == endpoint)
        {
            return Err(error(404, "Webhook is not active"));
        }
        self.start_run_context(run_id, &doc, payload, Mode::Webhook, now, context)?;
        Ok(json!({"runId":run_id}))
    }
    /// Active dataset users, for host-owned snapshots and receipt retention.
    pub fn data_runs(&self) -> Result<Vec<(String, Mode, Vec<String>)>, ApiError> {
        let mut result = Vec::new();
        for record in &self.runs {
            if !matches!(record.status()?, RunStatus::Running | RunStatus::Paused) {
                continue;
            }
            let run = record.restore()?;
            let ids = run
                .workflow()
                .nodes
                .iter()
                .filter(|n| connectors_data::handles(&n.kind))
                .map(|n| {
                    connectors_data::access(n)
                        .map(|a| a.table_id)
                        .map_err(|_| error(500, "Invalid stored data task"))
                })
                .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
            if !ids.is_empty() {
                result.push((run.id().to_string(), record.mode, ids.into_iter().collect()));
            }
        }
        Ok(result)
    }
    /// Data effects are completed synchronously by a host transaction, never claimed
    /// as uncertain network I/O. A cancelled/paused task cannot be dispatched.
    pub fn pending_data(&self, now: u64) -> Result<Option<(String, Task, Mode, u64)>, ApiError> {
        for record in &self.runs {
            if record.status()? != RunStatus::Running {
                continue;
            }
            if let Some(external) = record
                .external
                .iter()
                .chain(record.pending.iter())
                .find(|e| connectors_data::handles(&e.task.node.kind))
            {
                if connectors_data::handles(&external.task.node.kind) {
                    let time = if record.mode == Mode::Simulation {
                        record.logical_clock
                    } else {
                        now
                    };
                    return Ok(Some((
                        record.id_string()?,
                        external.task.clone(),
                        record.mode,
                        time,
                    )));
                }
            }
        }
        Ok(None)
    }
    pub fn complete_data(
        &mut self,
        id: &str,
        task_id: &str,
        outcome: Outcome,
        time: u64,
    ) -> Result<bool, ApiError> {
        let Some(record) = self.runs.iter_mut().find(|r| r.matches_id(id)) else {
            return Ok(false);
        };
        if record.status()? != RunStatus::Running {
            return Ok(false);
        }
        let Some(external) = record
            .external
            .iter_mut()
            .chain(record.pending.iter_mut())
            .find(|e| e.task.id == task_id)
        else {
            return Ok(false);
        };
        if external.task.id != task_id || !connectors_data::handles(&external.task.node.kind) {
            return Ok(false);
        }
        external.dispatched = true;
        self.complete_external(id, task_id, outcome, time)
    }
    /// Claim before network I/O and persist the returned task with its dispatched marker.
    pub fn claim_external(&mut self, now: u64) -> Result<Option<(String, Task)>, ApiError> {
        for record in &mut self.runs {
            if record.status()? != RunStatus::Running {
                continue;
            }
            let run_id = record.id_string()?;
            for external in record.external.iter_mut().chain(record.pending.iter_mut()) {
                if external.performable(now) {
                    external.dispatched = true;
                    record.wake_at = Some(
                        record
                            .wake_at
                            .unwrap_or(external.task.deadline_ms)
                            .min(external.task.deadline_ms),
                    );
                    return Ok(Some((run_id, external.task.clone())));
                }
            }
        }
        Ok(None)
    }
    /// Apply to the latest persisted state so pause/cancel cannot be overwritten by I/O.
    pub fn complete_external(
        &mut self,
        id: &str,
        task_id: &str,
        outcome: Outcome,
        now: u64,
    ) -> Result<bool, ApiError> {
        let Some(record) = self.runs.iter_mut().find(|r| r.matches_id(id)) else {
            return Ok(false);
        };
        if !record
            .external
            .iter()
            .chain(record.pending.iter())
            .find(|e| e.task.id == task_id)
            .is_some_and(|e| e.dispatched && e.task.id == task_id)
        {
            return Ok(false);
        }
        let now = if record.mode == Mode::Simulation {
            now.max(record.logical_clock)
        } else {
            now
        };
        let mut run = record.restore()?;
        if !matches!(run.status(), RunStatus::Running | RunStatus::Paused) {
            return Ok(false);
        }
        run.complete(task_id, outcome, now)
            .map_err(|e| error(500, e.to_string()))?;
        if record
            .external
            .as_ref()
            .is_some_and(|e| e.task.id == task_id)
        {
            record.external = None;
        }
        record.pending.retain(|e| e.task.id != task_id);
        record.store(&run, 500)?;
        record.advance(now)?;
        let simulation = record.mode == Mode::Simulation;
        let host_now = if simulation {
            self.runs
                .iter()
                .filter(|r| r.mode != Mode::Simulation)
                .map(|r| r.logical_clock)
                .max()
                .unwrap_or(0)
        } else {
            now
        };
        self.settle_invocations(host_now)?;
        self.record_audit(
            "engine",
            "runs.external_completed",
            id,
            "recorded",
            now,
            task_id,
        );
        Ok(true)
    }
    pub fn alarm(&mut self, now: u64) -> Result<(), ApiError> {
        // Only due runs and runs with open response windows can change here; every
        // other run's settle and advance would be a no-op.
        let due = self.due_runs(now)?;
        let before: Vec<_> = self
            .runs
            .iter()
            .filter(|r| due.iter().any(|id| r.matches_id(id)))
            .map(|r| Ok((r.id_string()?, r.status()?)))
            .collect::<Result<_, ApiError>>()?;
        for record in self
            .runs
            .iter_mut()
            .filter(|r| due.iter().any(|id| r.matches_id(id)))
        {
            record.settle_response(now)?;
            if record.wake_at.is_some_and(|at| at <= now)
                && let Err(_) = record.advance(now)
            {
                record.host_error = Some("engine_transition_failed".into());
                record.wake_at = None;
                record.external = None;
                record.pending.clear();
            }
        }
        for (id, status) in before {
            if let Some(record) = self.runs.iter().find(|r| r.matches_id(&id)) {
                if record.status()? != status {
                    self.record_audit("engine", "runs.state_changed", &id, "recorded", now, "");
                }
            }
        }
        self.settle_invocations(now)?;
        let due: Vec<_> = self
            .workflows
            .iter()
            .enumerate()
            .filter(|(_, w)| {
                w.status == Publication::Active && w.next_scheduled_at.is_some_and(|at| at <= now)
            })
            .map(|(i, w)| (i, w.clone()))
            .collect();
        for (index, draft) in due {
            let due_at = draft.next_scheduled_at.unwrap();
            let doc = self.live_document(&draft.graph.id)?;
            let id = format!("cron-{}-{}-{due_at}", doc.graph.id, doc.graph.revision);
            if self.find_run(&id)?.is_none() {
                let trigger = doc
                    .graph
                    .nodes
                    .iter()
                    .find(|n| n.kind == "Schedule")
                    .unwrap();
                let input = json!({"trigger":"schedule","scheduledAt":iso(due_at),"firedAt":iso(now),"cron":trigger.config.get("cron"),"timezone":trigger.config.get("timezone").cloned().unwrap_or(json!("UTC"))});
                // A full run slot is retried on a future alarm instead of losing its occurrence.
                if self
                    .start_run(&id, &doc, input, Mode::Schedule, now)
                    .is_err()
                {
                    self.workflows[index].next_scheduled_at = Some(now + 1000);
                    continue;
                }
            }
            self.record_audit("scheduler", "runs.started", &id, "succeeded", now, "");
            self.workflows[index].next_scheduled_at = schedule(&doc.graph, now)?.map(|s| s.at);
        }
        Ok(())
    }
}
#[derive(Debug)]
pub struct Schedule {
    pub at: u64,
}
pub fn schedule(workflow: &Workflow, after: u64) -> Result<Option<Schedule>, ApiError> {
    let Some(trigger) = workflow.nodes.iter().find(|n| n.kind == "Schedule") else {
        return Ok(None);
    };
    let expression = trigger
        .config
        .get("cron")
        .and_then(Value::as_str)
        .ok_or_else(|| error(400, "Schedule requires a cron expression"))?;
    if expression.split_whitespace().count() != 5 {
        return Err(error(400, "Schedule requires a five-field cron expression"));
    }
    let timezone: Tz = trigger
        .config
        .get("timezone")
        .and_then(Value::as_str)
        .unwrap_or("UTC")
        .parse()
        .map_err(|_| error(400, "Invalid IANA timezone"))?;
    let cron = Cron::from_str(expression).map_err(|_| error(400, "Invalid cron expression"))?;
    let next = cron
        .find_next_occurrence(&timestamp(after)?.with_timezone(&timezone), false)
        .map_err(|_| error(400, "No future cron occurrence"))?;
    Ok(Some(Schedule {
        at: next.timestamp_millis() as u64,
    }))
}

pub fn manifests() -> Vec<connector_sdk::NodeManifest> {
    let mut manifests = connectors_core::manifests();
    manifests.push(connector_sdk::NodeManifest{kind:"Each / Loop".into(),version:1,config_schema:json!({"type":"object","properties":{"workflowId":{"type":"string"},"revision":{"type":"integer","minimum":1},"arrayPointer":{"type":"string"},"concurrency":{"anyOf":[{"enum":[1,2,4]},{"type":"string","pattern":"^\\+?0*[124]$"}]},"failurePolicy":{"enum":["fail","collect"]}},"required":["workflowId"],"additionalProperties":false}),output_ports:vec!["success".into(),"error".into()],has_external_side_effects:true});

    manifests.push(connector_sdk::NodeManifest{kind:"Execute Workflow Trigger".into(),version:1,config_schema:json!({"type":"object","properties":{"outputNodeId":{"type":"string"},"inputSchema":{},"outputSchema":{}},"required":["outputNodeId"],"additionalProperties":false}),output_ports:vec!["success".into(),"error".into()],has_external_side_effects:false});
    manifests.push(connector_sdk::NodeManifest{kind:"Execute Workflow".into(),version:1,config_schema:json!({"type":"object","properties":{"workflowId":{"type":"string"},"revision":{"type":"integer","minimum":1},"mode":{"enum":["await","background"]},"inputPointer":{"type":"string"}},"required":["workflowId"],"additionalProperties":false}),output_ports:vec!["success".into(),"error".into()],has_external_side_effects:true});

    manifests.push(connectors_http::manifest());
    manifests.push(connectors_clairic::manifest());
    manifests.extend(connectors_email::manifests());
    manifests.extend(connectors_data::manifests());
    manifests.push(connector_sdk::NodeManifest{kind:"Respond to Webhook".into(),version:1,config_schema:json!({"type":"object","properties":{"status":{"oneOf":[{"type":"integer","minimum":200,"maximum":599},{"type":"string","description":"Numeric status or JSON-encoded $input reference"},{"type":"object","properties":{"$input":{"type":"string"}},"required":["$input"],"additionalProperties":false}],"default":200},"responseType":{"enum":["json","text"]},"headers":{"oneOf":[{"type":"object"},{"type":"string","description":"JSON-encoded response headers"}]},"body":{}},"additionalProperties":false}),output_ports:vec!["success".into(),"error".into()],has_external_side_effects:true});
    manifests.push(connector_sdk::NodeManifest {kind:"Request Context".into(),version:1,config_schema:json!({"type":"object","properties":{"part":{"enum":["metadata","body","raw_body","headers","query"]}},"additionalProperties":false}),output_ports:vec!["success".into(),"error".into()],has_external_side_effects:false});
    manifests
}
