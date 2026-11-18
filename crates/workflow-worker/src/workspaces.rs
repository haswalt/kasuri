//! The original workspace also owns the browser session's membership directory.
//! Workspace selection is per request, so switching tabs never changes a cookie.
use crate::{WorkflowWorkspace, api_error, internal};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use worker::*;

#[derive(Deserialize, Serialize)]
pub(crate) struct Membership {
    pub id: String,
    pub name: String,
}
impl WorkflowWorkspace {
    pub(crate) fn browser_workspace_key(&self) -> Result<String> {
        let sql = self.state.storage().sql();
        sql.exec(
            "CREATE TABLE IF NOT EXISTS workspace_identity (id INTEGER PRIMARY KEY CHECK(id=1))",
            None,
        )?;
        let named = !sql
            .exec("SELECT id FROM workspace_identity", None)?
            .to_array::<serde_json::Value>()?
            .is_empty();
        Ok(if named {
            self.state.id().to_string()
        } else {
            "personal".into()
        })
    }

    pub(crate) async fn workspace_route(
        &self,
        req: &mut Request,
        token: &str,
    ) -> Result<Option<Response>> {
        let url = req.url()?;
        let path = url.path();
        let selected = req.headers().get("X-Kasuri-Workspace")?.unwrap_or_default();
        // Only namespace stubs can set this: main always strips inbound delegation.
        if req.headers().get("X-Kasuri-Delegated")?.as_deref() == Some("1") {
            let sql = self.state.storage().sql();
            sql.exec("CREATE TABLE IF NOT EXISTS workspace_identity (id INTEGER PRIMARY KEY CHECK(id=1))", None)?;
            sql.exec(
                "INSERT OR IGNORE INTO workspace_identity(id) VALUES(1)",
                None,
            )?;
            return Ok(None);
        }
        let sql = self.state.storage().sql();
        sql.exec("CREATE TABLE IF NOT EXISTS workspace_memberships (id TEXT PRIMARY KEY, name TEXT NOT NULL)", None)?;
        sql.exec("INSERT OR IGNORE INTO workspace_memberships(id,name) VALUES('personal','Personal workspace')", None)?;
        if path == "/api/workspaces" || path.starts_with("/api/workspaces/") {
            if req.method() == Method::Get && path == "/api/workspaces" {
                let entries = sql
                    .exec(
                        "SELECT id,name FROM workspace_memberships ORDER BY rowid",
                        None,
                    )?
                    .to_array::<Membership>()?;
                return Ok(Some(Response::from_json(&json!({"workspaces":entries}))?));
            }
            if !((req.method() == Method::Post && path == "/api/workspaces")
                || req.method() == Method::Patch)
            {
                return Ok(Some(api_error(405, "Unsupported workspace operation")?));
            }
            let mut bytes = Vec::new();
            let mut stream = req.stream()?;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                if bytes.len() + chunk.len() > 1024 {
                    return Ok(Some(api_error(413, "Workspace request too large")?));
                }
                bytes.extend(chunk);
            }
            let value: serde_json::Value = match serde_json::from_slice(&bytes) {
                Ok(value) => value,
                Err(_) => return Ok(Some(api_error(400, "Invalid JSON")?)),
            };
            let name = value["name"].as_str().unwrap_or_default().trim();
            if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
                return Ok(Some(api_error(
                    400,
                    "Use a workspace name of 1–80 characters without control characters",
                )?));
            }
            if req.method() == Method::Patch {
                let id = path.strip_prefix("/api/workspaces/").unwrap_or_default();
                let member = sql
                    .exec(
                        "SELECT id,name FROM workspace_memberships WHERE id=?",
                        Some(vec![id.into()]),
                    )?
                    .to_array::<Membership>()?;
                if member.is_empty() {
                    return Ok(Some(api_error(404, "Workspace not found")?));
                }
                sql.exec(
                    "UPDATE workspace_memberships SET name=? WHERE id=?",
                    Some(vec![name.into(), id.into()]),
                )?;
                return Ok(Some(Response::from_json(&json!({"id":id,"name":name}))?));
            }
            // Authenticate the new workspace before publishing its directory entry.
            // A crash can leave an unreachable empty object, never a visible unowned one.
            let namespace = self.env.durable_object("WORKSPACES")?;
            let id = namespace.unique_id()?;
            let mut initialize =
                Request::new("https://workspace.internal/api/workspace", Method::Get)?;
            initialize.headers_mut()?.set("X-Kasuri-Session", token)?;
            initialize.headers_mut()?.set("X-Kasuri-Delegated", "1")?;
            let response = id.get_stub()?.fetch_with_request(initialize).await?;
            if response.status_code() != 200 {
                return Err(internal("Could not initialize workspace"));
            }
            let id = id.to_string();
            self.state.storage().sql().exec(
                "INSERT INTO workspace_memberships(id,name) VALUES(?,?)",
                Some(vec![id.clone().into(), name.into()]),
            )?;
            return Ok(Some(
                Response::from_json(&json!({"id":id,"name":name}))?.with_status(201),
            ));
        }
        if selected.is_empty() || selected == "personal" {
            return Ok(None);
        }
        let members = sql
            .exec(
                "SELECT id,name FROM workspace_memberships WHERE id=?",
                Some(vec![selected.clone().into()]),
            )?
            .to_array::<Membership>()?;
        if members.is_empty() {
            return Ok(Some(api_error(404, "Workspace not found")?));
        }
        let namespace = self.env.durable_object("WORKSPACES")?;
        let id = namespace.id_from_string(&selected)?;
        let mut forwarded = req.clone_mut()?;
        forwarded.headers_mut()?.set("X-Kasuri-Delegated", "1")?;
        forwarded.headers_mut()?.delete("X-Kasuri-Workspace")?;
        Ok(Some(id.get_stub()?.fetch_with_request(forwarded).await?))
    }
}
