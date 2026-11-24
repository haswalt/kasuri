//! Run objects: one Durable Object per run executed outside the workspace (see
//! "First deployment: run objects" in `docs/workspace-storage.md`).
//!
//! The object stores its run with a version and reports every save to its
//! workspace's index. Once the run has finished and the workspace has accepted
//! the final report, the object deletes its storage. How the run advances depends
//! on `RUN_EXECUTION`:
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
use worker::*;
use workflow_service::execution::{
    self, RunStatus, RunStore, SaveError, Step, WorkBus, WorkMessage,
};
use workflow_service::{ApiError, RunRecord};

const CREATE: &str = "CREATE TABLE IF NOT EXISTS run (id TEXT PRIMARY KEY, workspace TEXT NOT NULL, version INTEGER NOT NULL, body TEXT NOT NULL)";
/// How long to wait before retrying an unacknowledged final report.
const REPORT_RETRY_MS: u64 = 5_000;

/// The run, in this object's SQLite storage.
struct ObjectRun(SqlStorage);
#[derive(Deserialize)]
struct Row {
    body: String,
    version: f64,
    workspace: String,
}
impl ObjectRun {
    fn row(&self) -> std::result::Result<Option<Row>, ApiError> {
        self.0
            .exec(CREATE, None)
            .and_then(|_| {
                self.0
                    .exec("SELECT body, version, workspace FROM run", None)
            })
            .and_then(|c| c.to_array::<Row>())
            .map(|rows| rows.into_iter().next())
            .map_err(|e| failure(e.to_string()))
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
    fn load(&self, _run: &str) -> std::result::Result<Option<(RunRecord, u64)>, ApiError> {
        self.row()?
            .map(|row| {
                serde_json::from_str(&row.body)
                    .map(|record| (record, row.version as u64))
                    .map_err(|e| failure(e.to_string()))
            })
            .transpose()
    }
    fn save(
        &self,
        run: &str,
        record: &RunRecord,
        expected: Option<u64>,
    ) -> std::result::Result<u64, SaveError> {
        let body = serde_json::to_string(record).map_err(|e| SaveError::Failed(e.to_string()))?;
        let current = self
            .row()
            .map_err(|e| SaveError::Failed(e.message))?
            .map(|row| row.version as u64);
        if current != expected {
            return Err(SaveError::Conflict);
        }
        let version = expected.unwrap_or(0) + 1;
        // The workspace column is written at creation, below.
        self.0
            .exec(
                "UPDATE run SET body=?, version=? WHERE id=?",
                vec![body.into(), (version as i64).into(), run.into()],
            )
            .map_err(|e| SaveError::Failed(e.to_string()))?;
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

#[derive(Deserialize)]
struct Start {
    workspace: String,
    record: RunRecord,
}
#[derive(Deserialize)]
struct Control {
    action: String,
}
#[derive(Deserialize)]
struct Begin {
    version: u64,
}
#[derive(Serialize, Deserialize)]
struct Finish {
    task: String,
    outcome: connector_sdk::Outcome,
}
#[derive(Serialize, Deserialize)]
struct Claimed {
    task: Option<connector_sdk::Task>,
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
#[derive(Serialize)]
struct Report<'a> {
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
                let id = start.record.id_string().map_err(internal)?;
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
                    let record = start.record;
                    // `start` creates with expected `None`; the placeholder is
                    // version 0, so save over it explicitly.
                    execution::start(&Placeholder(&runs), &alarm, record, now())
                        .map_err(internal)?;
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
                let record: RunRecord = serde_json::from_str(&row.body).map_err(internal)?;
                let id = record.id_string().map_err(internal)?;
                match execution::control(&self.runs(), &alarm, &id, &control.action, now()) {
                    Ok((record, _)) => {
                        self.settle(&alarm).await?;
                        Response::from_json(&record.view(false).map_err(internal)?)
                    }
                    Err(error) => api_error(error.status, &error.message),
                }
            }
            (Method::Post, "/begin") => {
                let begin: Begin = req.json().await?;
                let runs = self.runs();
                let alarm = Alarm::default();
                let Some(row) = runs.row().map_err(internal)? else {
                    return Response::from_json(&Claimed { task: None });
                };
                if row.version as u64 == 0 {
                    return Response::from_json(&Claimed { task: None });
                }
                let record: RunRecord = serde_json::from_str(&row.body).map_err(internal)?;
                let message = WorkMessage {
                    run: record.id_string().map_err(internal)?,
                    version: begin.version,
                };
                let task =
                    match execution::begin(&runs, &alarm, &message, now()).map_err(internal)? {
                        Step::Done(_) => None,
                        Step::Perform { task, .. } => Some(task),
                    };
                self.settle(&alarm).await?;
                Response::from_json(&Claimed { task })
            }
            (Method::Post, "/finish") => {
                let finish: Finish = req.json().await?;
                let runs = self.runs();
                let alarm = Alarm::default();
                let Some(row) = runs.row().map_err(internal)? else {
                    return Response::from_json(&json!({"ok": true}));
                };
                let record: RunRecord = serde_json::from_str(&row.body).map_err(internal)?;
                let id = record.id_string().map_err(internal)?;
                execution::finish(&runs, &alarm, &id, &finish.task, finish.outcome, now())
                    .map_err(internal)?;
                self.settle(&alarm).await?;
                Response::from_json(&json!({"ok": true}))
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
                let record: RunRecord = serde_json::from_str(&row.body).map_err(internal)?;
                let message = WorkMessage {
                    run: record.id_string().map_err(internal)?,
                    version: row.version as u64,
                };
                match execution::begin(&runs, &alarm, &message, now()).map_err(internal)? {
                    Step::Done(_) => break,
                    Step::Perform { run, task } => {
                        let env = self.env.clone();
                        in_flight.push(async move {
                            let outcome = crate::http::execute(&task, &env).await;
                            (run, task, outcome)
                        });
                    }
                }
            }
            if in_flight.is_empty() {
                break;
            }
            // Report progress (for example, a claimed request) while waiting.
            self.report_current().await;
            let wait = alarm.0.get().unwrap_or(1_000).clamp(25, 1_000);
            let event = {
                let completion = Box::pin(in_flight.next());
                let timer = Box::pin(Delay::from(std::time::Duration::from_millis(wait)));
                match select(completion, timer).await {
                    Either::Left((value, _)) => value,
                    Either::Right(_) => None,
                }
            };
            if let Some((run, task, outcome)) = event {
                execution::finish(&runs, &alarm, &run, &task.id, outcome, now())
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
    fn load(&self, run: &str) -> std::result::Result<Option<(RunRecord, u64)>, ApiError> {
        self.0.load(run)
    }
    fn save(
        &self,
        run: &str,
        record: &RunRecord,
        expected: Option<u64>,
    ) -> std::result::Result<u64, SaveError> {
        self.0.save(run, record, expected.or(Some(0)))
    }
}

impl WorkflowRun {
    fn runs(&self) -> ObjectRun {
        ObjectRun(self.state.storage().sql())
    }
    /// After a step: report the saved run to the workspace, then arm the alarm
    /// for the next message, or, once the run has finished and the workspace has
    /// the final report, delete this object's storage.
    async fn settle(&self, alarm: &Alarm) -> Result<()> {
        let Some(row) = self.runs().row().map_err(internal)? else {
            return Ok(());
        };
        if row.version as u64 == 0 {
            return Ok(());
        }
        let record: RunRecord = serde_json::from_str(&row.body).map_err(internal)?;
        let reported = self.report(&row, &record).await;
        let finished = !matches!(
            record.status().map_err(internal)?,
            RunStatus::Running | RunStatus::Paused
        );
        let storage = self.state.storage();
        if finished {
            if reported {
                storage.delete_alarm().await?;
                storage.delete_all().await?;
            } else {
                storage
                    .set_alarm(std::time::Duration::from_millis(REPORT_RETRY_MS))
                    .await?;
            }
            return Ok(());
        }
        match alarm.0.get() {
            Some(delay) if self.queued()? && delay <= QUEUE_MAX_DELAY_MS => {
                // Queue delays are whole seconds: round up, never deliver early.
                let message = MessageBuilder::new(Envelope {
                    workspace: row.workspace.clone(),
                    run: record.id_string().map_err(internal)?,
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
    /// Report the stored run as it is now.
    async fn report_current(&self) {
        if let Ok(Some(row)) = self.runs().row()
            && row.version > 0.0
            && let Ok(record) = serde_json::from_str::<RunRecord>(&row.body)
        {
            self.report(&row, &record).await;
        }
    }
    /// Send the run's current state to its workspace's index. Reports are
    /// idempotent there (ignored unless newer), so a failed one is simply
    /// superseded by the next.
    async fn report(&self, row: &Row, record: &RunRecord) -> bool {
        let Ok(id) = record.id_string() else {
            return false;
        };
        let body = Report {
            run: &id,
            version: row.version as u64,
            body: &row.body,
        };
        let send = async {
            let namespace = self.env.durable_object("WORKSPACES")?;
            let stub = namespace.id_from_string(&row.workspace)?.get_stub()?;
            let mut init = RequestInit::new();
            init.with_method(Method::Post)
                .with_body(Some(serde_json::to_string(&body)?.into()));
            let request = Request::new_with_init("https://workspace/internal/runs/report", &init)?;
            stub.fetch_with_request(request).await
        };
        matches!(send.await, Ok(response) if response.status_code() == 200)
    }
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
    let outcome = crate::http::execute(&task, env).await;
    let finish = Finish {
        task: task.id,
        outcome,
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
