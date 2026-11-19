//! The Rust application layer shared by Cloudflare and self-hosted Wasm deployments.
//! All workspace data is persistent. No sample workflows, seed data or expiry mode.
mod audit;
mod http_api;
mod publication;
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
pub use webhook_response::{Reply as WebhookReply, ResponseWindow};
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    #[serde(default)]
    pub external: Option<ExternalTask>,
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
            json!({"id":run.id(),"workflowId":run.workflow().id,"name":run.workflow().name,"revision":run.workflow().revision,"createdAt":iso(self.created_at),"mode":self.mode,"status":self.status()?,"nodes":nodes,"webhookResponse":if summary {self.webhook_response.as_ref().map(|w|json!({"delivery":w.delivery,"deadlineMs":w.deadline_ms,"closedAt":w.closed_at}))}else{self.webhook_response.as_ref().map(|w|json!(w))},"context":if summary {Value::Null}else{self.trigger_context.clone()},"input":if summary {Value::Null}else{state["input"].clone()},"error":self.host_error.clone().map(Value::String).unwrap_or_else(||state["failure"].clone()),"wakeAt":self.wake_at}),
        )
    }
    fn advance(&mut self, now: u64) -> Result<(), ApiError> {
        self.settle_response(now)?;
        let mut run = self.restore()?;
        if let Some(external) = &self.external {
            if run.status() == RunStatus::Paused {
                self.wake_at = None;
                return Ok(());
            }
            if connectors_data::handles(&external.task.node.kind) {
                self.wake_at = Some(now.saturating_add(1));
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
            match run.next(clock).map_err(|e| error(400, e.to_string()))? {
                Dispatch::Task(task) => {
                    if connectors_data::handles(&task.node.kind)
                        || (task.node.kind == "HTTP Request" && self.mode != Mode::Simulation)
                    {
                        self.external = Some(ExternalTask {
                            task: *task,
                            dispatched: false,
                        });
                        self.wake_at = Some(now.saturating_add(1));
                        break;
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
                            _ => self.trigger_context.clone(),
                        })
                    } else if task.node.kind == "HTTP Request" {
                        connectors_http::simulate(&task)
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
        self.logical_clock = clock;
        self.checkpoint = run.checkpoint().map_err(|e| error(400, e.to_string()))?;
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
        let mut ws: Self =
            serde_json::from_str(text).map_err(|_| error(500, "Invalid stored workspace"))?;
        ws.migrate_publications();
        Ok(ws)
    }
    pub fn checkpoint(&mut self) -> Result<String, ApiError> {
        loop {
            let text =
                serde_json::to_string(self).map_err(|_| error(500, "Cannot encode workspace"))?;
            if text.len() <= MAX_STATE_BYTES && self.runs.len() <= 100 {
                return Ok(text);
            }
            let removable = (1..self.runs.len()).rev().find(|i| {
                !self.runs[*i]
                    .webhook_response
                    .as_ref()
                    .is_some_and(|w| w.open())
                    && self.runs[*i].status().is_ok_and(|s| {
                        matches!(
                            s,
                            RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
                        )
                    })
            });
            if let Some(index) = removable {
                self.runs.remove(index);
            } else if text.len() > MAX_STATE_BYTES && self.audit.len() > 1 {
                // Audit retention is bounded by both count and available storage.
                self.audit.remove(0);
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
            .chain(self.runs.iter().filter_map(|r| {
                r.webhook_response
                    .as_ref()
                    .filter(|w| w.open())
                    .and_then(|w| w.deadline_ms)
            }))
            .min()
    }
    pub fn snapshot(&self) -> Result<Value, ApiError> {
        Ok(
            json!({"apiVersion":2,"editPreconditionsRequired":true,"workflows":self.workflows.iter().filter(|w|w.archived_at.is_none()).collect::<Vec<_>>(),"runs":self.runs.iter().take(100).map(|r|r.view(true)).collect::<Result<Vec<_>,_>>()?,"connectors":manifests()}),
        )
    }
    fn index(&self, id: &str) -> Result<usize, ApiError> {
        self.workflows
            .iter()
            .position(|w| w.graph.id == id)
            .ok_or_else(|| error(404, "Workflow not found"))
    }
    pub fn executable(document: &Document) -> Result<(), ApiError> {
        document
            .graph
            .validate()
            .map_err(|e| error(400, e.to_string()))?;
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
        for node in &document.graph.nodes {
            if node.kind == "Validate Schema" {
                connectors_core::validation::validate(&node.config)
                    .map_err(|_| error(400, "Invalid or unsupported validation schema"))?;
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
                        !matches!(p.as_str(), Some("metadata" | "body" | "headers" | "query"))
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
                    _ => return Err(error(400, "Invalid webhook authentication configuration")),
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
        if trigger_context.to_string().len() > 16384 {
            return Err(error(413, "Trigger context exceeds budget"));
        }
        Self::executable(doc)?;
        if self
            .runs
            .iter()
            .filter(|r| {
                r.webhook_response.as_ref().is_some_and(|w| w.open())
                    || r.status()
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
        };
        record.advance(now)?;
        let view = record.view(false)?;
        self.runs.insert(0, record);
        Ok(view)
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
        if method == "GET" && path == "/api/audit" {
            return Ok((200, self.audit_page(0, 100)));
        }
        if method == "GET" && path == "/api/workspace" {
            return Ok((200, self.snapshot()?));
        }
        let parts: Vec<_> = path.trim_matches('/').split('/').collect();
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
                record.settle_response(now)?;
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
            if let Some(external) = &record.external {
                if connectors_data::handles(&external.task.node.kind) {
                    let time = if record.mode == Mode::Simulation {
                        record.logical_clock
                    } else {
                        now
                    };
                    return Ok(Some((
                        record.restore()?.id().into(),
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
        let Some(record) = self
            .runs
            .iter_mut()
            .find(|r| r.restore().is_ok_and(|run| run.id() == id))
        else {
            return Ok(false);
        };
        if record.status()? != RunStatus::Running {
            return Ok(false);
        }
        let Some(external) = &mut record.external else {
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
            let run_id = record.restore()?.id().to_string();
            if let Some(external) = &mut record.external {
                if !connectors_data::handles(&external.task.node.kind)
                    && !external.dispatched
                    && now < external.task.deadline_ms
                {
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
        let before: Vec<_> = self
            .runs
            .iter()
            .map(|r| Ok((r.restore()?.id().to_string(), r.status()?)))
            .collect::<Result<_, ApiError>>()?;
        for record in &mut self.runs {
            record.settle_response(now)?;
            if record.wake_at.is_some_and(|at| at <= now)
                && let Err(_) = record.advance(now)
            {
                record.host_error = Some("engine_transition_failed".into());
                record.wake_at = None;
                record.external = None;
            }
        }
        for (id, status) in before {
            if let Some(record) = self
                .runs
                .iter()
                .find(|r| r.restore().is_ok_and(|r| r.id() == id))
            {
                if record.status()? != status {
                    self.record_audit("engine", "runs.state_changed", &id, "recorded", now, "");
                }
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
        for (index, draft) in due {
            let due_at = draft.next_scheduled_at.unwrap();
            let doc = self.live_document(&draft.graph.id)?;
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
    manifests.push(connectors_http::manifest());
    manifests.extend(connectors_data::manifests());
    manifests.push(connector_sdk::NodeManifest{kind:"Respond to Webhook".into(),version:1,config_schema:json!({"type":"object","properties":{"status":{"type":"integer","default":200},"responseType":{"enum":["json","text"]},"headers":{"type":"object"},"body":{}},"additionalProperties":false}),output_ports:vec!["success".into(),"error".into()],has_external_side_effects:true});
    manifests.push(connector_sdk::NodeManifest {kind:"Request Context".into(),version:1,config_schema:json!({"type":"object","properties":{"part":{"enum":["metadata","body","headers","query"]}},"additionalProperties":false}),output_ports:vec!["success".into(),"error".into()],has_external_side_effects:false});
    manifests
}
