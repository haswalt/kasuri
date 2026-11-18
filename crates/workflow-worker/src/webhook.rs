//! Public webhook ingress is separate from browser management sessions.
use crate::{WorkflowWorkspace, api_error, now};
use connectors_webhook::{MAX_WEBHOOK_BYTES, bearer, hash, valid_event_id, verify_signature};
use futures_util::StreamExt;
use serde_json::{Value, json};
use worker::*;
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
            if !self.load()?.webhooks.is_empty() {
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
                    self.save(&mut workspace).await?;
                    if value["authentication"] == "bearer" {
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
                    value["path"] =
                        json!(format!("/api/hooks/{}/{id}/{endpoint}", self.state.id()));
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
        if parts.len() != 5
            || parts[0] != "api"
            || parts[1] != "hooks"
            || parts[2] != self.state.id().to_string()
        {
            return api_error(404, "Webhook not found");
        }
        if req.method() != Method::Post {
            return api_error(405, "Webhooks accept POST requests");
        }
        let event_id = req.headers().get("X-Kasuri-Event-Id")?.unwrap_or_default();
        if !valid_event_id(&event_id) {
            return api_error(400, "A valid X-Kasuri-Event-Id is required");
        }
        if !req
            .headers()
            .get("Content-Type")?
            .unwrap_or_default()
            .split(';')
            .next()
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
        {
            return api_error(415, "Webhook payload must be application/json");
        }
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
        let id = parts[3];
        let endpoint = parts[4];
        let registration = match workspace
            .webhooks
            .iter()
            .find(|w| w.workflow_id == id && w.endpoint_id == endpoint)
        {
            Some(r) => r,
            None => return api_error(404, "Webhook not found"),
        };
        let doc = match workspace.webhook_document(id) {
            Ok(doc) => doc,
            Err(_) => return api_error(404, "Webhook not found"),
        };
        if doc.status != workflow_service::Publication::Active {
            return api_error(404, "Webhook is not active");
        }
        let trigger = doc
            .graph
            .nodes
            .iter()
            .find(|n| n.kind == "Webhook")
            .unwrap();
        let authenticated = match trigger
            .config
            .get("authentication")
            .and_then(Value::as_str)
            .unwrap_or("bearer")
        {
            "bearer" => bearer(
                &req.headers().get("Authorization")?.unwrap_or_default(),
                &registration.token_hash,
            ),
            "hmac" => {
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
                secrets.get(alias).is_some_and(|secret| {
                    verify_signature(
                        secret.as_bytes(),
                        &req.headers()
                            .get("X-Kasuri-Timestamp")
                            .ok()
                            .flatten()
                            .unwrap_or_default(),
                        &event_id,
                        &body,
                        &req.headers()
                            .get("X-Kasuri-Signature")
                            .ok()
                            .flatten()
                            .unwrap_or_default(),
                        now() / 1000,
                    )
                })
            }
            _ => false,
        };
        if !authenticated {
            return api_error(401, "Webhook authentication failed");
        }
        let payload = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => return api_error(400, "Invalid JSON payload"),
        };
        let run_id = self
            .env
            .durable_object("WORKSPACES")?
            .unique_id()?
            .to_string();
        match workspace.receive_webhook(
            id,
            endpoint,
            &event_id,
            payload,
            &hash(&body),
            now(),
            &run_id,
        ) {
            Ok(value) => {
                self.save(&mut workspace).await?;
                let mut response = Response::from_json(&value)?.with_status(202);
                response.headers_mut().set("Cache-Control", "no-store")?;
                Ok(response)
            }
            Err(error) => api_error(error.status, &error.message),
        }
    }
}
