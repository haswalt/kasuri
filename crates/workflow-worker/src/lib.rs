//! HTTP, SQLite persistence and alarms, implemented directly with workers-rs.
mod http;
mod mcp;
mod storage;
mod webhook;
mod workspaces;
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use worker::*;
use workflow_service::MAX_BODY_BYTES;

fn now() -> u64 {
    Date::now().as_millis()
}
fn internal(error: impl std::fmt::Display) -> Error {
    Error::RustError(error.to_string())
}
fn api_error(status: u16, message: &str) -> Result<Response> {
    Ok(Response::from_json(&json!({"error":message}))?.with_status(status))
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    if url.path() == "/api/health" && req.method() == Method::Get {
        return Response::from_json(&json!({"ok":true,"engine":"rust-wasm","version":2}));
    }
    // Agent endpoints have their own bearer/OAuth authentication; browser cookies
    // and installation Basic credentials never authorize an MCP request.
    if let Some(workspace) = mcp::public_workspace(url.path()) {
        if workspace.len() != 64 || !workspace.bytes().all(|b| b.is_ascii_hexdigit()) {
            return api_error(404, "MCP workspace not found");
        }
        let namespace = env.durable_object("WORKSPACES")?;
        let id = match namespace.id_from_string(workspace) {
            Ok(id) => id,
            Err(_) => return api_error(404, "MCP workspace not found"),
        };
        return id.get_stub()?.fetch_with_request(req).await;
    }
    // Webhooks use their own event authentication and never inherit browser cookies.
    if url.path().starts_with("/api/hooks/") || url.path().starts_with("/hooks/") {
        let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
        let workspace = match parts.as_slice() {
            ["hooks", workspace, _] | ["api", "hooks", workspace, _, _] => *workspace,
            _ => return api_error(404, "Webhook not found"),
        };
        if workspace.len() != 64 || !workspace.bytes().all(|b| b.is_ascii_hexdigit()) {
            return api_error(404, "Webhook not found");
        }
        let namespace = env.durable_object("WORKSPACES")?;
        let id = match namespace.id_from_string(workspace) {
            Ok(id) => id,
            Err(_) => return api_error(404, "Webhook not found"),
        };
        return id.get_stub()?.fetch_with_request(req).await;
    }
    let credentials = env
        .secret("BASIC_AUTH")
        .map(|s| s.to_string())
        .or_else(|_| env.var("BASIC_AUTH").map(|v| v.to_string()))
        .unwrap_or_default();
    if !credentials.trim().is_empty() {
        let expected = format!("Basic {}", STANDARD.encode(credentials.trim()));
        let supplied = req.headers().get("Authorization")?.unwrap_or_default();
        if !bool::from(
            Sha256::digest(expected.as_bytes()).ct_eq(&Sha256::digest(supplied.as_bytes())),
        ) {
            let mut response = Response::error("Authentication required", 401)?;
            response.headers_mut().set(
                "WWW-Authenticate",
                "Basic realm=\"Kasuri\", charset=\"UTF-8\"",
            )?;
            return Ok(response);
        }
    }
    if !url.path().starts_with("/api/") {
        let path = url.path();
        let target = if path == "/" { "/index.html" } else { path };
        let mut asset_url = url.clone();
        asset_url.set_path(target);
        let assets = env.service("ASSETS")?;
        let mut response = assets
            .fetch_request(Request::new(asset_url.as_str(), req.method())?)
            .await?;
        if response.status_code() == 404
            && !path.rsplit('/').next().unwrap_or_default().contains('.')
        {
            asset_url.set_path("/index.html");
            response = assets
                .fetch_request(Request::new(asset_url.as_str(), req.method())?)
                .await?;
        }
        let headers = response.headers().clone();
        response = response.with_headers(headers);
        let content_type = response.headers().get("Content-Type")?;
        let mime = if content_type.is_none()
            || content_type.as_deref() == Some("application/octet-stream")
        {
            Some(match target.rsplit('.').next().unwrap_or("") {
                "js" | "mjs" => "text/javascript; charset=utf-8",
                "css" => "text/css; charset=utf-8",
                "svg" => "image/svg+xml",
                "png" => "image/png",
                "ico" => "image/x-icon",
                "woff2" => "font/woff2",
                "woff" => "font/woff",
                "ttf" => "font/ttf",
                "json" => "application/json",
                _ => "text/html; charset=utf-8",
            })
        } else {
            None
        };
        if let Some(mime) = mime {
            response.headers_mut().set("Content-Type", mime)?;
        }
        response
            .headers_mut()
            .set("X-Content-Type-Options", "nosniff")?;
        return Ok(response);
    }
    // A configured public origin is explicit trust for a TLS-terminating proxy;
    // never infer it from arbitrary X-Forwarded-* request headers.
    let configured_origin = env
        .var("PUBLIC_ORIGIN")
        .map(|v| v.to_string())
        .unwrap_or_default();
    let expected_origin = if configured_origin.is_empty() {
        url.origin().ascii_serialization()
    } else {
        if Url::parse(&configured_origin).ok().is_none_or(|v| {
            v.scheme() != "https" || v.origin().ascii_serialization() != configured_origin
        }) {
            return api_error(500, "PUBLIC_ORIGIN must be a canonical HTTPS origin");
        }
        configured_origin
    };
    if !matches!(req.method(), Method::Get | Method::Head) {
        let origin = req.headers().get("Origin")?;
        if origin
            .as_ref()
            .is_some_and(|o| o != expected_origin.as_str())
            || req.headers().get("Sec-Fetch-Site")?.as_deref() == Some("cross-site")
        {
            return api_error(403, "Cross-site mutation rejected");
        }
    }
    let namespace = env.durable_object("WORKSPACES")?;
    let cookie = req.headers().get("Cookie")?.unwrap_or_default();
    let session = cookie
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find(|(key, _)| *key == "kasuri_session")
        .map(|(_, value)| value);
    let mut session_parts = session.unwrap_or_default().split('.');
    let workspace_id = session_parts.next().unwrap_or_default();
    let existing =
        if workspace_id.len() == 64 && workspace_id.bytes().all(|b| b.is_ascii_hexdigit()) {
            namespace.id_from_string(workspace_id).ok()
        } else {
            None
        };
    let provided = session_parts
        .next()
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()));
    let fresh = provided.is_none() || existing.is_none();
    let session_token = match provided {
        Some(token) => token.to_string(),
        None => namespace.unique_id()?.to_string(),
    };
    let id = match existing {
        Some(id) => id,
        None => namespace.unique_id()?,
    };
    let mut forwarded = req.clone_mut()?;
    forwarded.headers_mut()?.delete("X-Kasuri-Delegated")?;
    forwarded
        .headers_mut()?
        .set("X-Kasuri-Session", &session_token)?;
    let mut response = id.get_stub()?.fetch_with_request(forwarded).await?;
    let headers = response.headers().clone();
    response = response.with_headers(headers);
    if fresh && response.status_code() < 400 {
        response.headers_mut().append(
            "Set-Cookie",
            &format!(
                "kasuri_session={id}.{session_token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=31536000{}",
                if expected_origin.starts_with("https://") {
                    "; Secure"
                } else {
                    ""
                }
            ),
        )?;
    }
    response.headers_mut().set("Cache-Control", "no-store")?;
    Ok(response)
}

#[durable_object]
pub struct WorkflowWorkspace {
    state: State,
    env: Env,
}
impl DurableObject for WorkflowWorkspace {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }
    async fn fetch(&self, mut req: Request) -> Result<Response> {
        let path = req.url()?.path().to_string();
        if mcp::public_workspace(&path).is_some() {
            return self.mcp_public(req).await;
        }
        if path.starts_with("/api/hooks/") || path.starts_with("/hooks/") {
            return self.incoming_webhook(req).await;
        }
        let token = req.headers().get("X-Kasuri-Session")?.unwrap_or_default();
        if !self.authorize_session(&token)? {
            return api_error(403, "Invalid workspace session");
        }
        if let Some(response) = self.workspace_route(&mut req, &token).await? {
            return Ok(response);
        }
        if path == "/api/mcp" || path.starts_with("/api/mcp/") {
            return self.mcp_management(req).await;
        }
        if let Some(response) = self.webhook_management(&path, req.method()).await? {
            return Ok(response);
        }
        let mut body = Vec::new();
        if !matches!(req.method(), Method::Get | Method::Head) {
            let mut stream = req.stream()?;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                if body.len().saturating_add(chunk.len()) > MAX_BODY_BYTES {
                    return api_error(413, "Request exceeds 128 KiB");
                }
                body.extend(chunk);
            }
        }
        // All body I/O precedes synchronous read/modify/save of the workspace.
        let mut workspace = self.load()?;
        let id = self
            .env
            .durable_object("WORKSPACES")?
            .unique_id()?
            .to_string();
        if path == "/api/audit" && req.method() == Method::Get {
            let url = req.url()?;
            let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            let after = pairs.get("after").and_then(|s| s.parse().ok()).unwrap_or(0);
            let limit = pairs
                .get("limit")
                .and_then(|s| s.parse().ok())
                .unwrap_or(100);
            if pairs.get("order").is_some_and(|v| v == "desc") {
                let before = pairs
                    .get("before")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                return Response::from_json(&workspace.audit_recent_page(before, limit));
            }
            return Response::from_json(&workspace.audit_page(after, limit));
        }
        let result = workspace.handle_http_v2(
            &req.method().to_string(),
            req.url()?.path(),
            &body,
            now(),
            &id,
        );
        match result {
            Ok((status, value)) => {
                self.save(&mut workspace).await?;
                Ok(Response::from_json(&value)?.with_status(status))
            }
            Err(error) => {
                self.save(&mut workspace).await?;
                api_error(error.status, &error.message)
            }
        }
    }
    async fn alarm(&self) -> Result<Response> {
        let mut workspace = self.load()?;
        workspace.alarm(now()).map_err(internal)?;
        let claimed = workspace.claim_external(now()).map_err(internal)?;
        self.save(&mut workspace).await?;
        if let Some((id, task)) = claimed {
            let outcome = http::execute(&task, &self.env).await;
            // Requests can pause or cancel a run while the upstream request is outstanding.
            let mut latest = self.load()?;
            latest
                .complete_external(&id, &task.id, outcome, now())
                .map_err(internal)?;
            self.save(&mut latest).await?;
        }
        Response::empty()
    }
}
