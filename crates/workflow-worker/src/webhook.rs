//! Public webhook ingress is separate from browser management sessions.
use crate::{WorkflowWorkspace, api_error, now};
use connectors_webhook::{MAX_WEBHOOK_BYTES, bearer, hash, verify_signature, verify_signature_v2};
use futures_util::StreamExt;
use serde_json::{Value, json};
use worker::*;
struct DisconnectObserver {
    signal: AbortSignal,
    callback: worker::wasm_bindgen::closure::Closure<dyn FnMut()>,
    disconnected: std::rc::Rc<std::cell::Cell<bool>>,
}
impl DisconnectObserver {
    fn new(signal: AbortSignal) -> Result<Self> {
        use worker::wasm_bindgen::JsCast;
        let disconnected = std::rc::Rc::new(std::cell::Cell::new(signal.aborted()));
        let flag = disconnected.clone();
        let callback = worker::wasm_bindgen::closure::Closure::wrap(
            Box::new(move || flag.set(true)) as Box<dyn FnMut()>,
        );
        signal.add_event_listener_with_callback("abort", callback.as_ref().unchecked_ref())?;
        Ok(Self {
            signal,
            callback,
            disconnected,
        })
    }
}
impl Drop for DisconnectObserver {
    fn drop(&mut self) {
        use worker::wasm_bindgen::JsCast;
        let _ = self
            .signal
            .remove_event_listener_with_callback("abort", self.callback.as_ref().unchecked_ref());
    }
}
impl WorkflowWorkspace {
    pub(crate) fn authorize_session(&self, token: &str) -> Result<bool> {
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(false);
        }
        let sql = self.state.storage().sql();
        sql.exec("CREATE TABLE IF NOT EXISTS session_auth (id INTEGER PRIMARY KEY CHECK(id=1), hash TEXT NOT NULL)",None)?;
        #[derive(serde::Deserialize)]
        struct Row {
            hash: String,
        }
        let rows = sql
            .exec("SELECT hash FROM session_auth WHERE id=1", None)?
            .to_array::<Row>()?;
        let digest = hash(token.as_bytes());
        if let Some(row) = rows.first() {
            use subtle::ConstantTimeEq;
            Ok(bool::from(row.hash.as_bytes().ct_eq(digest.as_bytes())))
        } else {
            // Fail closed if a partial restore kept public webhook routing but lost session auth.
            if {
                let workspace = self.load()?;
                !workspace.webhooks.is_empty()
                    || !workspace.workflows.is_empty()
                    || !workspace.management.is_null()
            } {
                return Ok(false);
            }
            sql.exec(
                "INSERT INTO session_auth(id,hash) VALUES(1,?)",
                Some(vec![digest.into()]),
            )?;
            Ok(true)
        }
    }
    pub(crate) async fn webhook_management(
        &self,
        path: &str,
        method: Method,
    ) -> Result<Option<Response>> {
        let parts: Vec<_> = path.trim_matches('/').split('/').collect();
        if parts.len() < 4 || parts[0] != "api" || parts[1] != "workflows" || parts[3] != "webhook"
        {
            return Ok(None);
        }
        let id = parts[2];
        let mut workspace = self.load()?;
        let result = if method == Method::Get && parts.len() == 4 {
            workspace.webhook_info(id)
        } else if method == Method::Post && parts.len() == 5 && parts[4] == "rotate" {
            let namespace = self.env.durable_object("WORKSPACES")?;
            let endpoint = namespace.unique_id()?.to_string();
            let token = namespace.unique_id()?.to_string();
            match workspace.configure_webhook(id, endpoint, hash(token.as_bytes())) {
                Ok(mut value) => {
                    let name = workspace
                        .workflows
                        .iter()
                        .find(|w| w.graph.id == id)
                        .map(|w| w.graph.name.clone())
                        .unwrap_or_else(|| id.into());
                    workspace.record_audit(
                        "browser",
                        "webhook.credential_rotated",
                        id,
                        "succeeded",
                        now(),
                        "",
                    );
                    if let Some(event) = workspace.audit.last_mut() {
                        event["entity"] = json!({"type":"workflow","id":id,"name":name});
                        event["summary"] = json!(format!("{name}: webhook credential rotated"));
                    }
                    self.save(&mut workspace).await?;
                    if matches!(value["authentication"].as_str(), Some("bearer" | "query"))
                        || workspace
                            .workflows
                            .iter()
                            .find(|w| w.graph.id == id)
                            .and_then(|w| w.graph.nodes.iter().find(|n| n.kind == "Webhook"))
                            .is_some_and(|n| {
                                matches!(
                                    n.config.get("authentication").and_then(Value::as_str),
                                    Some("bearer" | "query")
                                )
                            })
                    {
                        value["token"] = json!(token)
                    }
                    Ok(value)
                }
                Err(error) => Err(error),
            }
        } else {
            return Ok(Some(api_error(404, "Route not found")?));
        };
        let response = match result {
            Ok(mut value) => {
                if let Some(endpoint) = value["endpointId"].as_str() {
                    let endpoint = endpoint.to_string();
                    value["path"] = json!(format!("/hooks/{}/{endpoint}", self.state.id()));
                }
                Response::from_json(&value)?
            }
            Err(error) => api_error(error.status, &error.message)?,
        };
        Ok(Some(response))
    }
    pub(crate) async fn incoming_webhook(&self, mut req: Request) -> Result<Response> {
        let path = req.url()?.path().to_string();
        let parts: Vec<_> = path.trim_matches('/').split('/').collect();
        let own_id = self.state.id().to_string();
        let (workspace_id, legacy_workflow, endpoint) = match parts.as_slice() {
            ["hooks", workspace, endpoint] => (*workspace, None, *endpoint),
            ["api", "hooks", workspace, workflow, endpoint] => {
                (*workspace, Some(*workflow), *endpoint)
            }
            _ => return api_error(404, "Webhook not found"),
        };
        if workspace_id != own_id {
            return api_error(404, "Webhook not found");
        }
        if req.method() != Method::Post {
            return api_error(405, "Webhooks accept POST requests");
        }
        let content_type = req.headers().get("Content-Type")?.unwrap_or_default();
        let media_type = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let json_body = media_type == "application/json" || media_type.ends_with("+json");
        let mut body = Vec::new();
        let mut stream = req.stream()?;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if body.len() + chunk.len() > MAX_WEBHOOK_BYTES {
                return api_error(413, "Webhook payload exceeds 32 KiB");
            }
            body.extend(chunk);
        }
        // Reload after all network/body awaits. Authentication headers never enter workflow input.
        let mut workspace = self.load()?;
        let registration = match workspace.webhooks.iter().find(|w| {
            w.endpoint_id == endpoint && legacy_workflow.is_none_or(|id| w.workflow_id == id)
        }) {
            Some(r) => r,
            None => return api_error(404, "Webhook not found"),
        };
        let id = registration.workflow_id.clone();
        let id = id.as_str();
        let doc = match workspace.live_document(id) {
            Ok(doc) => doc,
            Err(_) => return api_error(404, "Webhook not found"),
        };
        if doc.status != workflow_service::Publication::Active {
            return api_error(404, "Webhook is not active");
        }
        let trigger = doc.graph.nodes.iter().find(|n| n.kind == "Webhook");
        let Some(trigger) = trigger else {
            return api_error(404, "Webhook is not active");
        };
        let authenticated = match trigger
            .config
            .get("authentication")
            .and_then(Value::as_str)
            .unwrap_or("bearer")
        {
            "none" => true,
            "query" => {
                let parameter = trigger
                    .config
                    .get("queryParameter")
                    .and_then(Value::as_str)
                    .unwrap_or("key");
                let url = req.url()?;
                let values: Vec<_> = url
                    .query_pairs()
                    .filter(|(key, _)| key == parameter)
                    .collect();
                values.len() == 1
                    && connectors_webhook::token(&values[0].1, &registration.token_hash)
            }
            "bearer" => bearer(
                &req.headers().get("Authorization")?.unwrap_or_default(),
                &registration.token_hash,
            ),
            mode @ ("hmac" | "hmac_sha256") => {
                let alias = trigger
                    .config
                    .get("signingSecret")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let raw = self
                    .env
                    .secret("HTTP_SECRETS")
                    .map(|s| s.to_string())
                    .or_else(|_| self.env.var("HTTP_SECRETS").map(|s| s.to_string()))
                    .unwrap_or_default();
                let secrets: std::collections::BTreeMap<String, String> =
                    serde_json::from_str(&raw).unwrap_or_default();
                let timestamp = req.headers().get("X-Kasuri-Timestamp")?.unwrap_or_default();
                let supplied = req.headers().get("X-Kasuri-Signature")?.unwrap_or_default();
                secrets.get(alias).is_some_and(|secret| {
                    if mode == "hmac" {
                        let event = req
                            .headers()
                            .get("X-Kasuri-Event-Id")
                            .ok()
                            .flatten()
                            .unwrap_or_default();
                        verify_signature(
                            secret.as_bytes(),
                            &timestamp,
                            &event,
                            &body,
                            &supplied,
                            now() / 1000,
                        )
                    } else {
                        verify_signature_v2(
                            secret.as_bytes(),
                            &timestamp,
                            &body,
                            &supplied,
                            now() / 1000,
                        )
                    }
                })
            }
            _ => false,
        };
        if !authenticated {
            return api_error(401, "Webhook authentication failed");
        }
        let mut context = match connectors_webhook::context::capture(
            &trigger.config,
            "POST",
            &path,
            req.headers().entries(),
            req.url()?
                .query_pairs()
                .map(|(key, value)| (key.into_owned(), value.into_owned())),
        ) {
            Ok(context) => context,
            Err(connectors_webhook::context::ContextError::BudgetExceeded) => {
                return api_error(413, "Selected request metadata exceeds context budget");
            }
            Err(_) => return api_error(400, "Invalid request context selection"),
        };
        if doc.graph.nodes.iter().any(|n| {
            n.kind == "Request Context"
                && n.config.get("part").and_then(serde_json::Value::as_str) == Some("raw_body")
        }) {
            let text = match std::str::from_utf8(&body) {
                Ok(text) => text,
                Err(_) => return api_error(400, "Webhook body must be UTF-8"),
            };
            context["rawBody"] = json!(text);
        }
        let text = match std::str::from_utf8(&body) {
            Ok(text) => text,
            Err(_) => return api_error(400, "Webhook body must be UTF-8"),
        };
        let payload = if json_body {
            serde_json::from_slice(&body).unwrap_or_else(|_| json!(text))
        } else {
            json!(text)
        };
        let run_id = self
            .env
            .durable_object("WORKSPACES")?
            .unique_id()?
            .to_string();
        match workspace.receive_webhook_context(id, endpoint, payload, now(), &run_id, context) {
            Ok(value) => {
                self.save(&mut workspace).await?;
                if workspace.runs.iter().any(|r| {
                    r.webhook_response
                        .as_ref()
                        .is_some_and(|w| w.deadline_ms.is_some())
                        && r.view(true).is_ok_and(|v| v["id"] == run_id)
                }) {
                    self.keep_webhook_response(&run_id, req.signal()).await
                } else {
                    let mut response = Response::from_json(&value)?.with_status(202);
                    response.headers_mut().set("Cache-Control", "no-store")?;
                    Ok(response)
                }
            }
            Err(error) => api_error(error.status, &error.message),
        }
    }
    async fn keep_webhook_response(&self, id: &str, signal: AbortSignal) -> Result<Response> {
        // The bounded responder outlives cancellation of its awaiting HTTP future,
        // so client disconnect can be recorded without cancelling the durable run.
        use worker::wasm_bindgen::JsCast;
        let raw: &worker::wasm_bindgen::JsValue = self.state.as_raw().as_ref();
        let host = WorkflowWorkspace {
            state: State::from(
                raw.clone()
                    .unchecked_into::<worker::worker_sys::DurableObjectState>(),
            ),
            env: self.env.clone(),
        };
        let id = id.to_string();
        let result = std::rc::Rc::new(std::cell::RefCell::new(None));
        let wake = std::rc::Rc::new(std::cell::RefCell::new(None::<std::task::Waker>));
        let output = result.clone();
        let notify = wake.clone();
        self.state.wait_until(async move {
            let response = host.await_webhook_response(&id, signal).await;
            *output.borrow_mut() = Some(response);
            if let Some(waker) = notify.borrow_mut().take() {
                waker.wake()
            }
        });
        futures_util::future::poll_fn(move |cx| {
            if let Some(result) = result.borrow_mut().take() {
                return std::task::Poll::Ready(result);
            }
            *wake.borrow_mut() = Some(cx.waker().clone());
            std::task::Poll::Pending
        })
        .await
    }
    async fn await_webhook_response(&self, id: &str, signal: AbortSignal) -> Result<Response> {
        let observer = DisconnectObserver::new(signal)?;
        loop {
            // Always reload: alarms and management requests can advance or cancel
            // this run while the original request is awaiting its response.
            let mut workspace = self.load()?;
            let reply = workspace
                .webhook_reply(
                    id,
                    now(),
                    observer.disconnected.get() || observer.signal.aborted(),
                )
                .map_err(crate::internal)?;
            if matches!(reply, workflow_service::WebhookReply::Pending) {
                Delay::from(std::time::Duration::from_millis(100)).await;
                continue;
            }
            // No await or network write occurs before the submission checkpoint.
            self.save(&mut workspace).await?;
            let mut response = match reply {
                workflow_service::WebhookReply::Response(spec) => {
                    let mut response = if matches!(spec.status, 204 | 205 | 304) {
                        Response::empty()?
                    } else if spec.response_type == "text" {
                        Response::ok(spec.body.as_str().unwrap_or_default())?
                    } else {
                        Response::from_json(&spec.body)?
                    };
                    response = response.with_status(spec.status);
                    response.headers_mut().set(
                        "Content-Type",
                        if spec.response_type == "text" {
                            "text/plain; charset=utf-8"
                        } else {
                            "application/json"
                        },
                    )?;
                    for (name, value) in spec.headers {
                        response.headers_mut().set(&name, &value)?;
                    }
                    response
                }
                workflow_service::WebhookReply::TimedOut => {
                    Response::from_json(&json!({"runId":id,"error":"webhook_response_timeout"}))?
                        .with_status(504)
                }
                workflow_service::WebhookReply::Disconnected => Response::empty()?.with_status(499),
                workflow_service::WebhookReply::NoResponse => Response::from_json(
                    &json!({"runId":id,"error":"workflow_completed_without_response"}),
                )?
                .with_status(502),
                workflow_service::WebhookReply::Failed => Response::from_json(
                    &json!({"runId":id,"error":"workflow_failed_before_response"}),
                )?
                .with_status(500),
                _ => {
                    Response::from_json(&json!({"runId":id,"error":"response_window_unavailable"}))?
                        .with_status(409)
                }
            };
            response.headers_mut().set("Cache-Control", "no-store")?;
            response
                .headers_mut()
                .set("X-Content-Type-Options", "nosniff")?;
            response.headers_mut().set("X-Kasuri-Run-Id", id)?;
            return Ok(response);
        }
    }
}
