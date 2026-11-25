//! HTTP management API v2: callers must carry the edit version they inspected.
//! Internal engine/fixture callers can still use `handle`; no public HTTP route does.
use crate::{ApiError, MAX_BODY_BYTES, Workspace, error};
use serde_json::Value;
impl Workspace {
    pub fn handle_http_v2(
        &mut self,
        method: &str,
        path: &str,
        body: &[u8],
        now: u64,
        run_id: &str,
    ) -> Result<(u16, Value), ApiError> {
        let check = (|| -> Result<(), ApiError> {
            if body.len() > MAX_BODY_BYTES {
                return Err(error(413, "Request exceeds 128 KiB"));
            }
            let parts: Vec<_> = path.trim_matches('/').split('/').collect();
            let workflow_write = parts.len() >= 3 && parts[0] == "api" && parts[1] == "workflows";
            let relevant = workflow_write
                && ((method == "PUT" && parts.len() == 3)
                    || (method == "DELETE" && parts.get(3..) == Some(&["trash"]))
                    || (method == "POST"
                        && (parts.len() == 4
                            && matches!(
                                parts[3],
                                "publish" | "pause" | "resume" | "archive" | "restore"
                            )
                            || parts.len() == 6
                                && parts[3] == "revisions"
                                && parts[5] == "restore")));
            if !relevant && !(method == "POST" && path == "/api/runs") {
                return Ok(());
            }
            let input: Value = if body.is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_slice(body).map_err(|_| error(400, "Invalid JSON"))?
            };
            let id = if workflow_write {
                parts[2]
            } else {
                input["workflowId"]
                    .as_str()
                    .ok_or_else(|| error(400, "Expected workflowId"))?
            };
            let index = match self.index(id) {
                Ok(index) => index,
                Err(_) if method == "PUT" => return Ok(()),
                Err(e) => return Err(e),
            };
            let field = if method == "PUT" {
                "version"
            } else {
                "expectedVersion"
            };
            let supplied=input.get(field).ok_or_else(||error(428,format!("{field} is required by HTTP API v2; reload the workflow and submit its edit version")))?;
            let expected = supplied
                .as_u64()
                .ok_or_else(|| error(400, format!("{field} must be a non-negative integer")))?;
            if self.workflows[index].version != expected {
                return Err(error(
                    409,
                    "Workflow changed; reload and review before retrying",
                ));
            }
            Ok(())
        })();
        if let Err(failure) = check {
            if let Some(id) = path
                .strip_prefix("/api/workflows/")
                .and_then(|p| p.split('/').next())
            {
                let before = self.workflows.iter().find(|w| w.graph.id == id).cloned();
                self.workflow_audit(
                    "browser",
                    "workflow.change_rejected",
                    id,
                    before.as_ref(),
                    before.as_ref(),
                    Some(failure.status),
                    now,
                    run_id,
                );
            }
            return Err(failure);
        }
        self.handle(method, path, body, now, run_id)
    }
}
