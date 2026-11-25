//! Run objects: one Durable Object per run tree (a top-level run and the child
//! runs it invokes) executed outside the workspace (see "First deployment: run
//! objects" in `docs/workspace-storage.md`).
//!
//! The object stores its tree with one version and reports the runs each save
//! changed to its workspace's index. Once every run has finished and the
//! workspace has accepted the final report, the object deletes its storage. How
//! the tree advances depends on `RUN_EXECUTION`:
//!
//! - `objects`: the object's alarm is the run's single live message. It advances
//!   the run with `workflow_service::execution` and performs claimed HTTP tasks
//!   itself.
//! - `queues`: the run's messages go on the `RUN_QUEUE` queue. Its consumer (any
//!   Worker invocation) asks the object to claim work for the message's version
//!   (`/begin`), performs the HTTP task itself, and reports the outcome
//!   (`/finish`). Delays longer than the queue allows wait on the object's alarm,
//!   which then sends the message.
use crate::{api_error, internal, now};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::cell::Cell;
use std::collections::HashMap;
use worker::*;
use workflow_service::execution::{self, RunStore, RunTree, SaveError, Step, WorkBus, WorkMessage};
use workflow_service::{ApiError, RunRecord};

/// The root run, with the tree's version (0 for a placeholder not yet started).
const CREATE: &str = "CREATE TABLE IF NOT EXISTS run (id TEXT PRIMARY KEY, workspace TEXT NOT NULL, version INTEGER NOT NULL, body TEXT NOT NULL)";
/// The root's descendants, in creation order (rowid), each with the tree version
/// that last changed it.
const CREATE_CHILD: &str = "CREATE TABLE IF NOT EXISTS child (id TEXT PRIMARY KEY, version INTEGER NOT NULL, body TEXT NOT NULL)";
/// The newest tree version the workspace has accepted a report for.
const CREATE_REPORTED: &str = "CREATE TABLE IF NOT EXISTS reported (version INTEGER NOT NULL)";
/// How long to wait before retrying an unacknowledged final report.
const REPORT_RETRY_MS: u64 = 5_000;

/// The run tree, in this object's SQLite storage.
struct ObjectRun(SqlStorage);
#[derive(Deserialize)]
struct Row {
    id: String,
    body: String,
    version: f64,
    workspace: String,
}
#[derive(Deserialize)]
struct ChildRow {
    id: String,
    body: String,
    version: f64,
}
impl ObjectRun {
    fn exec<T: serde::de::DeserializeOwned>(
        &self,
        sql: &str,
        params: Vec<SqlStorageValue>,
    ) -> std::result::Result<Vec<T>, ApiError> {
        self.0
            .exec(sql, Some(params))
            .and_then(|c| c.to_array::<T>())
            .map_err(|e| failure(e.to_string()))
    }
    fn create(&self) -> std::result::Result<(), ApiError> {
        for sql in [CREATE, CREATE_CHILD, CREATE_REPORTED] {
            self.0.exec(sql, None).map_err(|e| failure(e.to_string()))?;
        }
        Ok(())
    }
    /// The root row: the tree's ID, workspace and version.
    fn row(&self) -> std::result::Result<Option<Row>, ApiError> {
        self.create()?;
        Ok(self
            .exec("SELECT id, body, version, workspace FROM run", vec![])?
            .into_iter()
            .next())
    }
    /// The root's descendants, newest first (workspace order).
    fn children(&self) -> std::result::Result<Vec<ChildRow>, ApiError> {
        self.create()?;
        self.exec(
            "SELECT id, body, version FROM child ORDER BY rowid DESC",
            vec![],
        )
    }
    fn reported(&self) -> std::result::Result<u64, ApiError> {
        self.create()?;
        #[derive(Deserialize)]
        struct Reported {
            version: f64,
        }
        Ok(self
            .exec::<Reported>("SELECT version FROM reported", vec![])?
            .first()
            .map_or(0, |r| r.version as u64))
    }
    /// Reports can finish out of order; keep the newest.
    fn set_reported(&self, version: u64) -> std::result::Result<(), ApiError> {
        if self.reported()? >= version {
            return Ok(());
        }
        self.exec::<serde_json::Value>("DELETE FROM reported", vec![])?;
        self.exec::<serde_json::Value>(
            "INSERT INTO reported(version) VALUES(?)",
            vec![(version as i64).into()],
        )?;
        Ok(())
    }
    /// The tree as stored, with its version and workspace.
    fn tree(&self) -> std::result::Result<Option<(RunTree, Row)>, ApiError> {
        let Some(row) = self.row()? else {
            return Ok(None);
        };
        if row.version as u64 == 0 {
            return Ok(None);
        }
        let mut runs = self
            .children()?
            .into_iter()
            .map(|c| serde_json::from_str(&c.body).map_err(|e| failure(e.to_string())))
            .collect::<std::result::Result<Vec<RunRecord>, ApiError>>()?;
        runs.push(serde_json::from_str(&row.body).map_err(|e| failure(e.to_string()))?);
        Ok(Some((RunTree::new(runs), row)))
    }
}
fn failure(message: impl Into<String>) -> ApiError {
    ApiError {
        status: 500,
        message: message.into(),
        diagnostics: Vec::new(),
    }
}
impl RunStore for ObjectRun {
    fn load(&self, _root: &str) -> std::result::Result<Option<(RunTree, u64)>, ApiError> {
        Ok(self.tree()?.map(|(tree, row)| (tree, row.version as u64)))
    }
    fn save(
        &self,
        root: &str,
        tree: &RunTree,
        expected: Option<u64>,
    ) -> std::result::Result<u64, SaveError> {
        let failed = |e: ApiError| SaveError::Failed(e.message);
        let encode =
            |r: &RunRecord| serde_json::to_string(r).map_err(|e| SaveError::Failed(e.to_string()));
        let current = self.row().map_err(failed)?.map(|row| row.version as u64);
        if current != expected {
            return Err(SaveError::Conflict);
        }
        let version = expected.unwrap_or(0) + 1;
        let root_record = tree.root().map_err(failed)?;
        // The workspace column is written at creation, below. Every save bumps the
        // root's version; a child row is rewritten only when it changed.
        self.0
            .exec(
                "UPDATE run SET body=?, version=? WHERE id=?",
                vec![
                    encode(root_record)?.into(),
                    (version as i64).into(),
                    root.into(),
                ],
            )
            .map_err(|e| SaveError::Failed(e.to_string()))?;
        let stored: HashMap<String, String> = self
            .children()
            .map_err(failed)?
            .into_iter()
            .map(|c| (c.id, c.body))
            .collect();
        // Oldest first, so new children take rowids in creation order.
        for record in tree.runs.iter().rev().filter(|r| r.parent.is_some()) {
            let id = record.id_string().map_err(failed)?;
            let body = encode(record)?;
            if stored.get(&id) == Some(&body) {
                continue;
            }
            self.0
                .exec(
                    "INSERT INTO child(id, version, body) VALUES(?, ?, ?) ON CONFLICT(id) DO UPDATE SET version=excluded.version, body=excluded.body",
                    vec![id.into(), (version as i64).into(), body.into()],
                )
                .map_err(|e| SaveError::Failed(e.to_string()))?;
        }
        Ok(version)
    }
}

/// The object's alarm as a [`WorkBus`]: the latest message wins.
#[derive(Default)]
struct Alarm(Cell<Option<u64>>);
impl WorkBus for Alarm {
    fn send(
        &self,
        _message: WorkMessage,
        delay_ms: Option<u64>,
    ) -> std::result::Result<(), ApiError> {
        self.0.set(Some(delay_ms.unwrap_or(0)));
        Ok(())
    }
}

/// A tree to start: its runs newest first (root last), or (from an earlier
/// release) a single run.
#[derive(Deserialize)]
struct Start {
    workspace: String,
    #[serde(default)]
    record: Option<RunRecord>,
    #[serde(default)]
    runs: Vec<RunRecord>,
}
#[derive(Deserialize)]
struct Control {
    action: String,
    /// The run in the tree; the root if absent.
    #[serde(default)]
    run: Option<String>,
}
#[derive(Deserialize)]
struct Begin {
    version: u64,
}
#[derive(Serialize, Deserialize)]
struct Finish {
    /// The run in the tree that claimed the task; the root if absent.
    #[serde(default)]
    run: Option<String>,
    task: String,
    outcome: connector_sdk::Outcome,
    /// For a data task, the time it was performed at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    time: Option<u64>,
}
#[derive(Serialize, Deserialize)]
struct Claimed {
    #[serde(default)]
    run: Option<String>,
    task: Option<connector_sdk::Task>,
    /// Set for a data task: perform it on the workspace's data at this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    time: Option<u64>,
}
#[derive(Deserialize)]
struct ReplyRequest {
    run: String,
    #[serde(default)]
    disconnected: bool,
}
/// A queue message: this run has work at this version.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub workspace: String,
    pub run: String,
    pub version: u64,
}
/// The longest delay a queue message can have (24 hours); longer waits use the
/// run object's alarm.
const QUEUE_MAX_DELAY_MS: u64 = 24 * 60 * 60 * 1000;
/// The runs of a tree that changed since the last accepted report.
#[derive(Serialize)]
struct Report<'a> {
    root: &'a str,
    runs: Vec<Reported<'a>>,
}
#[derive(Serialize)]
struct Reported<'a> {
    run: &'a str,
    version: u64,
    body: &'a str,
}

#[durable_object]
pub struct WorkflowRun {
    state: State,
    env: Env,
}
impl DurableObject for WorkflowRun {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }
    async fn fetch(&self, mut req: Request) -> Result<Response> {
        let path = req.url()?.path().to_string();
        match (req.method(), path.as_str()) {
            (Method::Post, "/start") => {
                let start: Start = req.json().await?;
                let runs = self.runs();
                let tree = RunTree::new(match start.record {
                    Some(record) => vec![record],
                    None => start.runs,
                });
                let id = tree.root_id().map_err(internal)?;
                let sql = self.state.storage().sql();
                sql.exec(CREATE, None)?;
                sql.exec(
                    "INSERT OR IGNORE INTO run(id, workspace, version, body) VALUES(?, ?, 0, '')",
                    vec![id.clone().into(), start.workspace.into()],
                )?;
                // Version 0 is the placeholder row; starting it again is a no-op.
                let alarm = Alarm::default();
                let fresh = runs
                    .row()
                    .map_err(internal)?
                    .is_some_and(|row| row.version as u64 == 0);
                if fresh {
                    // `start` creates with expected `None`; the placeholder is
                    // version 0, so save over it explicitly.
                    execution::start(&Placeholder(&runs), &alarm, tree, now()).map_err(internal)?;
                }
                self.settle(&alarm).await?;
                Response::from_json(&json!({"ok": true}))
            }
            (Method::Post, "/control") => {
                let control: Control = req.json().await?;
                let alarm = Alarm::default();
                let Some(row) = self.runs().row().map_err(internal)? else {
                    return api_error(404, "Run not found");
                };
                let run = control.run.unwrap_or_else(|| row.id.clone());
                match execution::control(
                    &self.runs(),
                    &alarm,
                    &row.id,
                    &run,
                    &control.action,
                    now(),
                ) {
                    Ok((record, _)) => {
                        // The caller answers from the index: report before replying.
                        self.settle_reported(&alarm).await?;
                        Response::from_json(&record.view(false).map_err(internal)?)
                    }
                    Err(error) => api_error(error.status, &error.message),
                }
            }
            (Method::Post, "/begin") => {
                let begin: Begin = req.json().await?;
                let runs = self.runs();
                let alarm = Alarm::default();
                let none = Claimed {
                    run: None,
                    task: None,
                    time: None,
                };
                let Some(row) = runs.row().map_err(internal)? else {
                    return Response::from_json(&none);
                };
                if row.version as u64 == 0 {
                    return Response::from_json(&none);
                }
                let message = WorkMessage {
                    run: row.id,
                    version: begin.version,
                };
                let claimed =
                    match execution::begin(&runs, &alarm, &message, now()).map_err(internal)? {
                        Step::Done(_) => none,
                        Step::Perform { run, task } => Claimed {
                            run: Some(run),
                            task: Some(task),
                            time: None,
                        },
                        Step::Data { run, task, time } => Claimed {
                            run: Some(run),
                            task: Some(task),
                            time: Some(time),
                        },
                    };
                self.settle(&alarm).await?;
                Response::from_json(&claimed)
            }
            (Method::Post, "/finish") => {
                let finish: Finish = req.json().await?;
                let runs = self.runs();
                let alarm = Alarm::default();
                let Some(row) = runs.row().map_err(internal)? else {
                    return Response::from_json(&json!({"ok": true}));
                };
                let run = finish.run.unwrap_or_else(|| row.id.clone());
                match finish.time {
                    Some(time) => execution::finish_data(
                        &runs,
                        &alarm,
                        &row.id,
                        &run,
                        &finish.task,
                        finish.outcome,
                        time,
                        now(),
                    ),
                    None => execution::finish(
                        &runs,
                        &alarm,
                        &row.id,
                        &run,
                        &finish.task,
                        finish.outcome,
                        now(),
                    ),
                }
                .map_err(internal)?;
                self.settle(&alarm).await?;
                Response::from_json(&json!({"ok": true}))
            }
            (Method::Post, "/reply") => {
                // The workspace's handler holding this run's webhook request asks
                // for its response, as it would ask the workspace.
                let request: ReplyRequest = req.json().await?;
                let runs = self.runs();
                let alarm = Alarm::default();
                let Some(row) = runs.row().map_err(internal)? else {
                    return api_error(404, "Run not found");
                };
                let reply = match execution::reply(
                    &runs,
                    &alarm,
                    &row.id,
                    &request.run,
                    now(),
                    request.disconnected,
                ) {
                    Ok(Some(reply)) => reply,
                    Ok(None) => return api_error(404, "Run not found"),
                    Err(error) => return api_error(error.status, &error.message),
                };
                if reply != workflow_service::WebhookReply::Pending {
                    self.settle(&alarm).await?;
                }
                Response::from_json(&reply)
            }
            _ => api_error(404, "Route not found"),
        }
    }
    async fn alarm(&self) -> Result<Response> {
        if self.queued()? {
            // With queues the alarm only carries delays longer than the queue
            // allows (and final-report retries): send the message it held.
            let alarm = Alarm::default();
            alarm.0.set(Some(0));
            self.settle(&alarm).await?;
            return Response::empty();
        }
        use futures_util::{
            StreamExt,
            future::{Either, select},
            stream::FuturesUnordered,
        };
        let runs = self.runs();
        let alarm = Alarm::default();
        let mut in_flight = FuturesUnordered::new();
        // Like the workspace: claim every ready task, perform them concurrently
        // (up to 8), and keep advancing due work while they are outstanding.
        loop {
            while in_flight.len() < 8 {
                let Some(row) = runs.row().map_err(internal)? else {
                    break;
                };
                if row.version as u64 == 0 {
                    break;
                }
                let message = WorkMessage {
                    run: row.id,
                    version: row.version as u64,
                };
                match execution::begin(&runs, &alarm, &message, now()).map_err(internal)? {
                    Step::Done(_) => break,
                    Step::Data { run, task, time } => {
                        // Data tasks are short transactions on the workspace:
                        // performed in turn, not alongside requests.
                        let outcome =
                            perform_data(&self.env, &row.workspace, &run, &task, time).await?;
                        execution::finish_data(
                            &runs,
                            &alarm,
                            &message.run,
                            &run,
                            &task.id,
                            outcome,
                            time,
                            now(),
                        )
                        .map_err(internal)?;
                    }
                    Step::Perform { run, task } => {
                        let env = self.env.clone();
                        let workspace = row.workspace.clone();
                        in_flight.push(async move {
                            let outcome = crate::external::execute(&task, &env, &workspace).await;
                            (run, task, outcome)
                        });
                    }
                }
            }
            if in_flight.is_empty() {
                break;
            }
            // Report progress (for example, a claimed request) without holding
            // up the requests.
            self.report_later();
            let wait = alarm.0.get().unwrap_or(1_000).clamp(25, 1_000);
            let event = {
                let completion = Box::pin(in_flight.next());
                let timer = Box::pin(Delay::from(std::time::Duration::from_millis(wait)));
                match select(completion, timer).await {
                    Either::Left((value, _)) => value,
                    Either::Right(_) => None,
                }
            };
            if let Some((run, task, outcome)) = event
                && let Some(row) = runs.row().map_err(internal)?
            {
                execution::finish(&runs, &alarm, &row.id, &run, &task.id, outcome, now())
                    .map_err(internal)?;
            }
        }
        self.settle(&alarm).await?;
        Response::empty()
    }
}

/// Saves over the version-0 placeholder row for the first save.
struct Placeholder<'a>(&'a ObjectRun);
impl RunStore for Placeholder<'_> {
    fn load(&self, run: &str) -> std::result::Result<Option<(RunTree, u64)>, ApiError> {
        self.0.load(run)
    }
    fn save(
        &self,
        run: &str,
        record: &RunTree,
        expected: Option<u64>,
    ) -> std::result::Result<u64, SaveError> {
        self.0.save(run, record, expected.or(Some(0)))
    }
}

impl WorkflowRun {
    fn runs(&self) -> ObjectRun {
        ObjectRun(self.state.storage().sql())
    }
    /// After a step: report the saved tree to the workspace, then arm the alarm
    /// for the next message, or, once every run has finished and the workspace
    /// has the final report, delete this object's storage.
    async fn settle(&self, alarm: &Alarm) -> Result<()> {
        self.settle_with(alarm, false).await
    }
    /// As `settle`, but the report is applied before this returns.
    async fn settle_reported(&self, alarm: &Alarm) -> Result<()> {
        self.settle_with(alarm, true).await
    }
    async fn settle_with(&self, alarm: &Alarm, report_now: bool) -> Result<()> {
        let Some((tree, row)) = self.runs().tree().map_err(internal)? else {
            return Ok(());
        };
        let finished = !tree.active().map_err(internal)?;
        let storage = self.state.storage();
        if finished {
            // Storage goes only once the workspace has the final state.
            if self.report(&row).await {
                storage.delete_alarm().await?;
                storage.delete_all().await?;
            } else {
                storage
                    .set_alarm(std::time::Duration::from_millis(REPORT_RETRY_MS))
                    .await?;
            }
            return Ok(());
        }
        // Intermediate reports never hold up the next step: they are idempotent,
        // and the index may lag by a step.
        if report_now {
            self.report(&row).await;
        } else {
            self.report_later();
        }
        match alarm.0.get() {
            Some(delay) if self.queued()? && delay <= QUEUE_MAX_DELAY_MS => {
                // Queue delays are whole seconds: round up, never deliver early.
                let message = MessageBuilder::new(Envelope {
                    workspace: row.workspace.clone(),
                    run: row.id.clone(),
                    version: row.version as u64,
                })
                .delay_seconds(delay.div_ceil(1000) as u32)
                .content_type(QueueContentType::Json)
                .build();
                self.env.queue("RUN_QUEUE")?.send(message).await?;
                storage.delete_alarm().await?;
            }
            Some(delay) => {
                storage
                    .set_alarm(std::time::Duration::from_millis(delay))
                    .await?
            }
            // Nothing scheduled (for example, paused): no message until a control
            // request changes that.
            None => storage.delete_alarm().await?,
        }
        Ok(())
    }
    /// Whether runs advance through the queue (`RUN_EXECUTION=queues`).
    fn queued(&self) -> Result<bool> {
        Ok(crate::storage::parse_run_execution(
            &self
                .env
                .var("RUN_EXECUTION")
                .map(|v| v.to_string())
                .unwrap_or_default(),
        )
        .map_err(internal)?
            == crate::storage::RunExecution::Queues)
    }
    /// Report the stored tree as it is now, in the background.
    fn report_later(&self) {
        if let Ok(Some(row)) = self.runs().row()
            && row.version > 0.0
            && let Some(report) = self.prepare_report(&row)
        {
            let (env, sql) = (self.env.clone(), self.state.storage().sql());
            self.state.wait_until(async move {
                send_report(env, sql, report).await;
            });
        }
    }
    /// Report the stored tree and wait for the workspace to accept it.
    async fn report(&self, row: &Row) -> bool {
        match self.prepare_report(row) {
            Some(report) => send_report(self.env.clone(), self.state.storage().sql(), report).await,
            None => false,
        }
    }
    /// The runs that changed since the last accepted report: always the root,
    /// and children saved since. Reports are idempotent in the workspace (ignored
    /// unless newer), so a failed one is simply superseded by the next, which
    /// includes everything it had.
    fn prepare_report(&self, row: &Row) -> Option<PreparedReport> {
        let runs = self.runs();
        let (Ok(children), Ok(since)) = (runs.children(), runs.reported()) else {
            return None;
        };
        let version = row.version as u64;
        let body = Report {
            root: &row.id,
            // Oldest first: a child's parent is in the index before the child.
            runs: std::iter::once(Reported {
                run: &row.id,
                version,
                body: &row.body,
            })
            .chain(
                children
                    .iter()
                    .rev()
                    .filter(|c| c.version as u64 > since)
                    .map(|c| Reported {
                        run: &c.id,
                        version: c.version as u64,
                        body: &c.body,
                    }),
            )
            .collect(),
        };
        Some(PreparedReport {
            workspace: row.workspace.clone(),
            body: serde_json::to_string(&body).ok()?,
            version,
        })
    }
}

/// A report ready to send to the workspace's index.
struct PreparedReport {
    workspace: String,
    body: String,
    version: u64,
}
/// Send a report; on acceptance, remember its version so later reports only
/// carry children changed since.
async fn send_report(env: Env, sql: SqlStorage, report: PreparedReport) -> bool {
    let send = async {
        let namespace = env.durable_object("WORKSPACES")?;
        let stub = namespace.id_from_string(&report.workspace)?.get_stub()?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_body(Some(report.body.into()));
        let request = Request::new_with_init("https://workspace/internal/runs/report", &init)?;
        stub.fetch_with_request(request).await
    };
    let accepted = matches!(send.await, Ok(response) if response.status_code() == 200);
    accepted && ObjectRun(sql).set_reported(report.version).is_ok()
}

/// Process one queue message: ask the run object to claim work for its version,
/// perform the claimed HTTP task here (in the consumer, not the object), and
/// report the outcome. Safe to repeat: the object drops stale versions and
/// outcomes for tasks it isn't waiting on.
pub(crate) async fn consume(envelope: &Envelope, env: &Env) -> Result<()> {
    let stub = env
        .durable_object("RUNS")?
        .id_from_name(&format!("{}/{}", envelope.workspace, envelope.run))?
        .get_stub()?;
    let call = |path: &str, body: String| -> Result<Request> {
        let mut init = RequestInit::new();
        init.with_method(Method::Post).with_body(Some(body.into()));
        Request::new_with_init(&format!("https://run{path}"), &init)
    };
    let mut response = stub
        .fetch_with_request(call(
            "/begin",
            serde_json::to_string(&json!({"version": envelope.version}))?,
        )?)
        .await?;
    if response.status_code() != 200 {
        return Err(internal(format!(
            "run object refused a step: {}",
            response.status_code()
        )));
    }
    let claimed: Claimed = response.json().await?;
    let Some(task) = claimed.task else {
        return Ok(());
    };
    let outcome = match claimed.time {
        Some(time) => {
            let run = claimed.run.as_deref().unwrap_or(&envelope.run);
            perform_data(env, &envelope.workspace, run, &task, time).await?
        }
        None => crate::external::execute(&task, env, &envelope.workspace).await,
    };
    let finish = Finish {
        run: claimed.run,
        task: task.id,
        outcome,
        time: claimed.time,
    };
    let response = stub
        .fetch_with_request(call("/finish", serde_json::to_string(&finish)?)?)
        .await?;
    if response.status_code() != 200 {
        return Err(internal(format!(
            "run object refused an outcome: {}",
            response.status_code()
        )));
    }
    Ok(())
}

/// Perform a data task on its workspace's data: the workspace applies it in a
/// transaction, keyed by the task's idempotency key, so a repeat replays it.
async fn perform_data(
    env: &Env,
    workspace: &str,
    run: &str,
    task: &connector_sdk::Task,
    time: u64,
) -> Result<connector_sdk::Outcome> {
    let stub = env
        .durable_object("WORKSPACES")?
        .id_from_string(workspace)?
        .get_stub()?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(
        serde_json::to_string(&json!({"run": run, "task": task, "time": time}))?.into(),
    ));
    let mut response = stub
        .fetch_with_request(Request::new_with_init(
            "https://workspace/internal/runs/data",
            &init,
        )?)
        .await?;
    if response.status_code() != 200 {
        return Err(internal(format!(
            "the workspace refused a data task: {}",
            response.status_code()
        )));
    }
    response.json().await
}
