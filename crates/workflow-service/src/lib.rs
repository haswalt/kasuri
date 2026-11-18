//! The Rust application layer shared by Cloudflare and self-hosted Wasm deployments.
//! All workspace data is persistent. No sample workflows, seed data or expiry mode.
use chrono::{DateTime, SecondsFormat, Utc};
use chrono_tz::Tz;
use connector_sdk::{Connector, FailureCode, Outcome, Task};
use connectors_core::CoreConnector;
use croner::Cron;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::str::FromStr;
use workflow_engine::{Dispatch, Limits, NodeState, Run, RunStatus};
use workflow_schema::Workflow;

pub const MAX_STATE_BYTES: usize = 1_500_000;
pub const MAX_BODY_BYTES: usize = 131_072;
#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
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
    #[serde(flatten)]
    pub graph: Workflow,
    pub status: Publication,
    pub updated_at: String,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    #[serde(default)]
    pub external: Option<ExternalTask>,
    pub checkpoint: String,
    pub mode: Mode,
    pub created_at: u64,
    pub wake_at: Option<u64>,
    pub host_error: Option<String>,
}
impl RunRecord {
    fn restore(&self) -> Result<Run, ApiError> {
        Run::restore(&self.checkpoint).map_err(|_| error(500, "Invalid stored run checkpoint"))
    }
    pub fn status(&self) -> Result<RunStatus, ApiError> {
        Ok(if self.host_error.is_some() {
            RunStatus::Failed
        } else {
            self.restore()?.status()
        })
    }
    pub fn view(&self, summary: bool) -> Result<Value, ApiError> {
        let run = self.restore()?;
        let state: Value =
            serde_json::from_str(&self.checkpoint).map_err(|_| error(500, "Invalid stored run"))?;
        let nodes:Vec<_>=run.workflow().nodes.iter().map(|n| {
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
        }).collect();
        Ok(
            json!({"id":run.id(),"workflowId":run.workflow().id,"name":run.workflow().name,"revision":run.workflow().revision,"createdAt":iso(self.created_at),"mode":self.mode,"status":self.status()?,"nodes":nodes,"input":if summary {Value::Null}else{state["input"].clone()},"error":self.host_error.clone().map(Value::String).unwrap_or_else(||state["failure"].clone()),"wakeAt":self.wake_at}),
        )
    }
    fn advance(&mut self, now: u64) -> Result<(), ApiError> {
        let mut run = self.restore()?;
        if let Some(external) = &self.external {
            if run.status() == RunStatus::Paused {
                self.wake_at = None;
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
        let mut clock = now;
        self.wake_at = None;
        let budget = if self.mode == Mode::Simulation {
            1000
        } else {
            20
        };
        for _ in 0..budget {
            match run.next(clock).map_err(|e| error(400, e.to_string()))? {
                Dispatch::Task(task) => {
                    if task.node.kind == "HTTP Request" && self.mode != Mode::Simulation {
                        self.external = Some(ExternalTask {
                            task: *task,
                            dispatched: false,
                        });
                        self.wake_at = Some(clock.saturating_add(1));
                        break;
                    }
                    let outcome = if task.node.kind == "HTTP Request" {
                        connectors_http::simulate(&task)
                    } else {
                        CoreConnector.execute(&task)
                    };
                    run.complete(&task.id, outcome, clock)
                        .map_err(|e| error(400, e.to_string()))?;
                }
                Dispatch::SleepUntil(at) => {
                    if self.mode == Mode::Simulation {
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
        self.checkpoint = run.checkpoint().map_err(|e| error(400, e.to_string()))?;
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
    #[serde(default)]
    pub webhooks: Vec<WebhookRegistration>,
    #[serde(default)]
    pub webhook_events: Vec<WebhookEvent>,
    pub workflows: Vec<Document>,
    pub runs: Vec<RunRecord>,
}
impl Workspace {
    pub fn from_json(text: &str) -> Result<Self, ApiError> {
        if text.len() > MAX_STATE_BYTES {
            return Err(error(500, "Stored workspace exceeds size limit"));
        }
        serde_json::from_str(text).map_err(|_| error(500, "Invalid stored workspace"))
    }
    pub fn checkpoint(&mut self) -> Result<String, ApiError> {
        loop {
            let text =
                serde_json::to_string(self).map_err(|_| error(500, "Cannot encode workspace"))?;
            if text.len() <= MAX_STATE_BYTES && self.runs.len() <= 100 {
                return Ok(text);
            }
            let removable = (1..self.runs.len()).rev().find(|i| {
                self.runs[*i].status().is_ok_and(|s| {
                    matches!(
                        s,
                        RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
                    )
                })
            });
            if let Some(index) = removable {
                self.runs.remove(index);
            } else {
                return Err(error(
                    413,
                    "Workspace checkpoint budget exceeded; reduce stored inputs/outputs",
                ));
            }
        }
    }
    pub fn next_alarm(&self) -> Option<u64> {
        self.workflows
            .iter()
            .filter_map(|w| w.next_scheduled_at)
            .chain(self.runs.iter().filter_map(|r| r.wake_at))
            .min()
    }
    pub fn snapshot(&self) -> Result<Value, ApiError> {
        Ok(
            json!({"workflows":self.workflows,"runs":self.runs.iter().take(100).map(|r|r.view(true)).collect::<Result<Vec<_>,_>>()?,"connectors":manifests()}),
        )
    }
    fn index(&self, id: &str) -> Result<usize, ApiError> {
        self.workflows
            .iter()
            .position(|w| w.graph.id == id)
            .ok_or_else(|| error(404, "Workflow not found"))
    }
    fn executable(document: &Document) -> Result<(), ApiError> {
        document
            .graph
            .validate()
            .map_err(|e| error(400, e.to_string()))?;
        let manifests = manifests();
        for node in &document.graph.nodes {
            if node.kind == "Webhook" {
                match node
                    .config
                    .get("authentication")
                    .and_then(Value::as_str)
                    .unwrap_or("bearer")
                {
                    "bearer" => {}
                    "hmac"
                        if node
                            .config
                            .get("signingSecret")
                            .and_then(Value::as_str)
                            .is_some_and(|s| !s.is_empty()) => {}
                    _ => {
                        return Err(error(
                            400,
                            "Webhook requires bearer authentication or hmac with a signingSecret alias",
                        ));
                    }
                }
            }
            if node.kind == "HTTP Request" {
                let mut node = node.clone();
                node.config.remove("query");
                node.config.remove("body");
                connectors_http::validate(&Task{id:"validate".into(),idempotency_key:"validate".into(),node,input:Value::Null,attempt:1,deadline_ms:1}).map_err(|_|error(400,"Invalid HTTP request: use HTTPS, a supported auth configuration and one attempt for writes"))?;
            }
            if !manifests
                .iter()
                .any(|m| m.kind == node.kind && m.version == node.version)
            {
                return Err(error(400, format!("{} is not implemented", node.kind)));
            }
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
        Self::executable(doc)?;
        if self
            .runs
            .iter()
            .filter(|r| {
                r.status()
                    .is_ok_and(|s| matches!(s, RunStatus::Running | RunStatus::Paused))
            })
            .count()
            >= 5
        {
            return Err(error(
                429,
                "Wait for an active run to finish before starting another",
            ));
        }
        let run = Run::new(id, doc.graph.clone(), input, now, Limits::default())
            .map_err(|e| error(400, e.to_string()))?;
        let mut record = RunRecord {
            external: None,
            checkpoint: run.checkpoint().map_err(|e| error(400, e.to_string()))?,
            mode,
            created_at: now,
            wake_at: None,
            host_error: None,
        };
        record.advance(now)?;
        let view = record.view(false)?;
        self.runs.insert(0, record);
        Ok(view)
    }
    pub fn handle(
        &mut self,
        method: &str,
        path: &str,
        body: &[u8],
        now: u64,
        run_id: &str,
    ) -> Result<(u16, Value), ApiError> {
        if body.len() > MAX_BODY_BYTES {
            return Err(error(413, "Request exceeds 128 KiB"));
        }
        if method == "GET" && path == "/api/workspace" {
            return Ok((200, self.snapshot()?));
        }
        let parts: Vec<_> = path.trim_matches('/').split('/').collect();
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
        if parts[1] == "workflows" && method == "PUT" && parts.len() == 3 {
            let mut doc: Document = serde_json::from_slice(body)
                .map_err(|_| error(400, "Invalid workflow document"))?;
            if doc.graph.id != id
                || id.len() > 160
                || doc.graph.schema_version != 1
                || doc.graph.name.len() > 160
                || doc.graph.nodes.len() > 100
                || doc.graph.edges.len() > 500
            {
                return Err(error(400, "Invalid workflow metadata or graph size"));
            }
            doc.status = Publication::Draft;
            doc.updated_at = iso(now);
            doc.next_scheduled_at = None;
            if let Ok(index) = self.index(id) {
                doc.graph.revision = self.workflows[index].graph.revision;
                self.workflows[index] = doc.clone();
            } else {
                doc.graph.revision = 0;
                self.workflows.push(doc.clone());
            }
            return Ok((200, serde_json::to_value(doc).unwrap()));
        }
        if parts[1] == "workflows" && method == "POST" && parts.len() == 4 {
            let index = self.index(id)?;
            let mut doc = self.workflows[index].clone();
            match parts[3] {
                "pause" => {
                    doc.status = Publication::Paused;
                    doc.next_scheduled_at = None;
                }
                "publish" | "resume" => {
                    Self::executable(&doc)?;
                    doc.next_scheduled_at = schedule(&doc.graph, now)?.map(|s| s.at);
                    doc.status = Publication::Active;
                    if parts[3] == "publish" {
                        doc.graph.revision = doc
                            .graph
                            .revision
                            .checked_add(1)
                            .ok_or_else(|| error(409, "Revision limit reached"))?
                    }
                }
                _ => return Err(error(404, "Unknown workflow action")),
            }
            doc.updated_at = iso(now);
            self.workflows[index] = doc.clone();
            return Ok((200, serde_json::to_value(doc).unwrap()));
        }
        if parts[1] == "runs" {
            let index = self
                .runs
                .iter()
                .position(|r| r.restore().is_ok_and(|r| r.id() == id))
                .ok_or_else(|| error(404, "Run not found"))?;
            if method == "GET" && parts.len() == 3 {
                return Ok((200, self.runs[index].view(false)?));
            }
            if method == "POST" && parts.len() == 4 {
                let record = &mut self.runs[index];
                let mut run = record.restore()?;
                match parts[3] {
                    "pause" => run.pause(),
                    "resume" => run.resume(),
                    "cancel" => run.cancel(),
                    _ => return Err(error(404, "Unknown run action")),
                }
                .map_err(|e| error(409, e.to_string()))?;
                record.checkpoint = run.checkpoint().map_err(|e| error(400, e.to_string()))?;
                record.wake_at = None;
                if parts[3] == "cancel" {
                    record.external = None;
                }
                if parts[3] == "resume" {
                    record.advance(now)?
                }
                return Ok((200, record.view(false)?));
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
        let trigger = doc
            .graph
            .nodes
            .iter()
            .find(|n| n.kind == "Webhook")
            .unwrap();
        Ok(
            json!({"configured":registration.is_some(),"endpointId":registration.map(|r|&r.endpoint_id),"active":doc.status==Publication::Active,"authentication":trigger.config.get("authentication").and_then(Value::as_str).unwrap_or("bearer")}),
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
    pub fn receive_webhook(
        &mut self,
        id: &str,
        endpoint: &str,
        event_id: &str,
        payload: Value,
        fingerprint: &str,
        now: u64,
        run_id: &str,
    ) -> Result<Value, ApiError> {
        let doc = self.webhook_document(id)?.clone();
        if doc.status != Publication::Active
            || !self
                .webhooks
                .iter()
                .any(|w| w.workflow_id == id && w.endpoint_id == endpoint)
        {
            return Err(error(404, "Webhook is not active"));
        }
        if !connectors_webhook::valid_event_id(event_id) {
            return Err(error(400, "A valid X-Kasuri-Event-Id is required"));
        }
        self.webhook_events
            .retain(|e| now.saturating_sub(e.received_at) < 86_400_000);
        if let Some(existing) = self
            .webhook_events
            .iter()
            .find(|e| e.endpoint_id == endpoint && e.event_id == event_id)
        {
            if existing.fingerprint != fingerprint {
                return Err(error(
                    409,
                    "Event ID was already used with a different payload",
                ));
            }
            return Ok(json!({"runId":existing.run_id,"eventId":event_id,"duplicate":true}));
        }
        if self.webhook_events.len() >= 1000 {
            return Err(error(
                429,
                "Webhook deduplication window is full; retry later",
            ));
        }
        self.start_run(run_id, &doc, payload, Mode::Webhook, now)?;
        self.webhook_events.push(WebhookEvent {
            endpoint_id: endpoint.into(),
            event_id: event_id.into(),
            fingerprint: fingerprint.into(),
            run_id: run_id.into(),
            received_at: now,
        });
        Ok(json!({"runId":run_id,"eventId":event_id,"duplicate":false}))
    }
    /// Claim before network I/O and persist the returned task with its dispatched marker.
    pub fn claim_external(&mut self, now: u64) -> Result<Option<(String, Task)>, ApiError> {
        for record in &mut self.runs {
            if record.status()? != RunStatus::Running {
                continue;
            }
            let run_id = record.restore()?.id().to_string();
            if let Some(external) = &mut record.external {
                if !external.dispatched && now < external.task.deadline_ms {
                    external.dispatched = true;
                    record.wake_at = Some(external.task.deadline_ms);
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
        let Some(record) = self
            .runs
            .iter_mut()
            .find(|r| r.restore().is_ok_and(|run| run.id() == id))
        else {
            return Ok(false);
        };
        if !record
            .external
            .as_ref()
            .is_some_and(|e| e.dispatched && e.task.id == task_id)
        {
            return Ok(false);
        }
        let mut run = record.restore()?;
        if !matches!(run.status(), RunStatus::Running | RunStatus::Paused) {
            return Ok(false);
        }
        run.complete(task_id, outcome, now)
            .map_err(|e| error(500, e.to_string()))?;
        record.external = None;
        record.checkpoint = run.checkpoint().map_err(|e| error(500, e.to_string()))?;
        record.advance(now)?;
        Ok(true)
    }
    pub fn alarm(&mut self, now: u64) -> Result<(), ApiError> {
        for record in &mut self.runs {
            if record.wake_at.is_some_and(|at| at <= now)
                && let Err(_) = record.advance(now)
            {
                record.host_error = Some("engine_transition_failed".into());
                record.wake_at = None;
                record.external = None;
            }
        }
        let due: Vec<_> = self
            .workflows
            .iter()
            .enumerate()
            .filter(|(_, w)| {
                w.status == Publication::Active && w.next_scheduled_at.is_some_and(|at| at <= now)
            })
            .map(|(i, w)| (i, w.clone()))
            .collect();
        for (index, doc) in due {
            let due_at = doc.next_scheduled_at.unwrap();
            let id = format!("cron-{}-{}-{due_at}", doc.graph.id, doc.graph.revision);
            if !self
                .runs
                .iter()
                .any(|r| r.restore().is_ok_and(|run| run.id() == id))
            {
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
    manifests.push(connectors_http::manifest());
    manifests
}
