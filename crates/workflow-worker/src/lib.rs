//! HTTP, SQLite persistence and alarms, implemented directly with workers-rs.
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use worker::*;
use workflow_service::{MAX_BODY_BYTES, Workspace};

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
        return Response::from_json(&json!({"ok":true,"engine":"rust-wasm","version":1}));
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
    if !matches!(req.method(), Method::Get | Method::Head) {
        let origin = req.headers().get("Origin")?;
        if origin
            .as_ref()
            .is_some_and(|o| o != url.origin().ascii_serialization().as_str())
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
    let existing = session
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .and_then(|s| namespace.id_from_string(s).ok());
    let fresh = existing.is_none();
    let id = match existing {
        Some(id) => id,
        None => namespace.unique_id()?,
    };
    let mut response = id.get_stub()?.fetch_with_request(req).await?;
    let headers = response.headers().clone();
    response = response.with_headers(headers);
    if fresh {
        response.headers_mut().append(
            "Set-Cookie",
            &format!(
                "kasuri_session={id}; HttpOnly; SameSite=Strict; Path=/; Max-Age=31536000{}",
                if url.scheme() == "https" {
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
impl WorkflowWorkspace {
    fn load(&self) -> Result<Workspace> {
        let sql = self.state.storage().sql();
        sql.exec("CREATE TABLE IF NOT EXISTS workspace (id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL)",None)?;
        #[derive(Deserialize)]
        struct Row {
            body: String,
        }
        let rows = sql
            .exec("SELECT body FROM workspace WHERE id=1", None)?
            .to_array::<Row>()?;
        rows.first()
            .map(|r| Workspace::from_json(&r.body).map_err(internal))
            .unwrap_or_else(|| Ok(Workspace::default()))
    }
    async fn save(&self, workspace: &mut Workspace) -> Result<()> {
        let body = workspace.checkpoint().map_err(internal)?;
        self.state.storage().sql().exec("INSERT INTO workspace(id,body) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET body=excluded.body",Some(vec![body.into()]))?;
        if let Some(at) = workspace.next_alarm() {
            self.state
                .storage()
                .set_alarm(std::time::Duration::from_millis(at.saturating_sub(now())))
                .await?;
        } else {
            self.state.storage().delete_alarm().await?;
        }
        Ok(())
    }
}
impl DurableObject for WorkflowWorkspace {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }
    async fn fetch(&self, mut req: Request) -> Result<Response> {
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
        let result = workspace.handle(
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
            Err(error) => api_error(error.status, &error.message),
        }
    }
    async fn alarm(&self) -> Result<Response> {
        let mut workspace = self.load()?;
        workspace.alarm(now()).map_err(internal)?;
        self.save(&mut workspace).await?;
        Response::empty()
    }
}
