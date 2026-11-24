//! Versioned record storage. One INSERT publishes a complete generation atomically;
//! readers never combine rows from different saves. Legacy checkpoints migrate on save.
use crate::{WorkflowWorkspace, internal};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
use worker::wasm_bindgen::{JsCast, JsValue, closure::Closure};
use worker::*;
use workflow_service::Workspace;

const CREATE_RECORDS: &str = "CREATE TABLE IF NOT EXISTS workspace_records (generation INTEGER NOT NULL, kind TEXT NOT NULL, position INTEGER NOT NULL, body TEXT NOT NULL, PRIMARY KEY(generation,kind,position))";
const STORE_RECORDS: &str = r#"
WITH input(body,generation) AS (
  SELECT json(?), COALESCE((SELECT MAX(generation) FROM workspace_records),0)+1
), records(kind,position,body) AS (
  SELECT 'metadata',0,json_set(json_remove(body,'$.workflows','$.runs','$.audit','$.webhooks','$.webhookEvents','$.revisions'),'$.storageVersion',1) FROM input
  UNION ALL
  SELECT CASE collection.key
    WHEN 'workflows' THEN 'workflow' WHEN 'runs' THEN 'run'
    WHEN 'audit' THEN 'audit' WHEN 'webhooks' THEN 'webhook'
    WHEN 'webhookEvents' THEN 'webhook_event' WHEN 'revisions' THEN 'revision' END,
    CAST(item.key AS INTEGER),item.value
  FROM input,json_each(input.body) AS collection,
    json_each(CASE WHEN collection.key IN ('workflows','runs','audit','webhooks','webhookEvents','revisions') THEN collection.value ELSE '[]' END) AS item
)
INSERT INTO workspace_records(generation,kind,position,body)
SELECT input.generation,records.kind,records.position,records.body FROM input,records
"#;
const LOAD_RECORDS: &str = "SELECT kind,body FROM workspace_records WHERE generation=(SELECT MAX(generation) FROM workspace_records) ORDER BY kind,position";
#[derive(Deserialize)]
struct Record {
    kind: String,
    body: String,
}
fn decode(rows: Vec<Record>) -> Result<Workspace> {
    let mut root = None;
    let mut collections = json!({"workflows":[],"runs":[],"audit":[],"webhooks":[],"webhookEvents":[],"revisions":[]});
    let mut encoded = 0;
    for row in rows {
        encoded += row.body.len();
        let value: Value = serde_json::from_str(&row.body).map_err(internal)?;
        let field = match row.kind.as_str() {
            "metadata" => {
                if root.is_some() || value["storageVersion"] != 1 || !value.is_object() {
                    return Err(internal("Invalid workspace record metadata"));
                }
                root = Some(value);
                continue;
            }
            "revision" => "revisions",
            "workflow" => "workflows",
            "run" => "runs",
            "audit" => "audit",
            "webhook" => "webhooks",
            "webhook_event" => "webhookEvents",
            _ => return Err(internal("Unknown workspace record type")),
        };
        collections[field].as_array_mut().unwrap().push(value);
    }
    let mut root = root.ok_or_else(|| internal("Workspace record metadata missing"))?;
    root.as_object_mut().unwrap().remove("storageVersion");
    let Value::Object(collections) = collections else {
        unreachable!()
    };
    for (key, value) in collections {
        root[key] = value;
    }
    // Each record is parsed once; the assembled value is not re-encoded and
    // re-parsed as text.
    Workspace::from_value(root, encoded).map_err(internal)
}
fn write_encoded_records(sql: &SqlStorage, body: String) -> Result<()> {
    sql.exec(CREATE_RECORDS, None)?;
    sql.exec(STORE_RECORDS, Some(vec![body.into()]))?;
    sql.exec("DELETE FROM workspace_records WHERE generation < (SELECT MAX(generation) FROM workspace_records)",None)?;
    sql.exec("DELETE FROM workspace WHERE id=1", None)?;
    Ok(())
}
pub(crate) fn write_workspace_records(sql: &SqlStorage, workspace: &mut Workspace) -> Result<()> {
    let body = workspace.checkpoint().map_err(internal)?;
    crate::data::sync_run_data(sql, workspace, crate::now())?;
    write_encoded_records(sql, body)?;
    write_webhook_auth(sql, workspace)
}
pub(crate) const CREATE_WEBHOOK_AUTH: &str = "CREATE TABLE IF NOT EXISTS webhook_auth (endpoint_id TEXT NOT NULL, workflow_id TEXT NOT NULL, body TEXT NOT NULL, PRIMARY KEY(endpoint_id,workflow_id))";
/// Rewritten in the same transaction as every workspace generation, so it always
/// describes the committed workspace. Incoming webhooks reject bad credentials from
/// it without loading the workspace.
fn write_webhook_auth(sql: &SqlStorage, workspace: &Workspace) -> Result<()> {
    sql.exec(CREATE_WEBHOOK_AUTH, None)?;
    sql.exec("DELETE FROM webhook_auth", None)?;
    for auth in workspace.webhook_auth() {
        let body = serde_json::to_string(&auth).map_err(internal)?;
        sql.exec(
            "INSERT OR REPLACE INTO webhook_auth(endpoint_id,workflow_id,body) VALUES(?,?,?)",
            Some(vec![
                auth.endpoint_id.into(),
                auth.workflow_id.into(),
                body.into(),
            ]),
        )?;
    }
    Ok(())
}
impl WorkflowWorkspace {
    /// workers-rs 0.9 exposes SQL but not transactionSync. Bind the documented
    /// synchronous storage method in Rust; never fall back to non-atomic writes.
    pub(crate) fn sql_transaction<T: 'static>(
        &self,
        operation: impl FnOnce(SqlStorage) -> Result<T> + 'static,
    ) -> Result<T> {
        let raw = self.state.as_raw().storage().map_err(Error::from)?;
        let method = js_sys::Reflect::get(raw.as_ref(), &JsValue::from_str("transactionSync"))?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| internal("Synchronous SQLite transactions are unavailable"))?;
        let slot = Rc::new(RefCell::new(None));
        let output = slot.clone();
        let sql = self.state.storage().sql();
        let callback =
            Closure::once_assert_unwind_safe(move || -> std::result::Result<JsValue, JsValue> {
                let value = operation(sql);
                let failed = value.is_err();
                *output.borrow_mut() = Some(value);
                if failed {
                    Err(JsValue::from_str("SQLite transaction aborted"))
                } else {
                    Ok(JsValue::NULL)
                }
            });
        let committed = method.call1(raw.as_ref(), callback.as_ref());
        let result = slot.borrow_mut().take();
        match (committed, result) {
            (Ok(_), Some(value)) => value,
            (Err(_), Some(Err(error))) => Err(error),
            (Err(error), _) => Err(Error::from(error)),
            _ => Err(internal("SQLite transaction callback did not run")),
        }
    }

    /// The committed workspace with this host's run limits applied.
    pub(crate) fn load(&self) -> Result<Workspace> {
        let mut workspace = self.load_stored()?;
        workspace.max_active_runs = self.max_active_runs()?;
        Ok(workspace)
    }
    /// `MAX_ACTIVE_RUNS` from the environment: unset, empty, `0` or `unlimited`
    /// set no count limit. An invalid value fails requests rather than silently
    /// falling back to a different limit.
    fn max_active_runs(&self) -> Result<Option<usize>> {
        let value = self
            .env
            .var("MAX_ACTIVE_RUNS")
            .map(|v| v.to_string())
            .unwrap_or_default();
        workflow_service::parse_max_active_runs(&value).map_err(internal)
    }
    fn load_stored(&self) -> Result<Workspace> {
        let sql = self.state.storage().sql();
        sql.exec(CREATE_RECORDS, None)?;
        #[derive(Deserialize)]
        struct Generation {
            generation: Option<f64>,
        }
        let generation = sql
            .exec(
                "SELECT MAX(generation) AS generation FROM workspace_records",
                None,
            )?
            .one::<Generation>()?
            .generation;
        if let Some(generation) = generation {
            // Every save inserts a new generation and a rolled-back transaction
            // leaves the old one, so a decoded copy of this generation is exactly
            // the committed workspace.
            if let Some((cached, workspace)) = &*self.cache.borrow() {
                if *cached == generation {
                    return Ok(workspace.clone());
                }
            }
            // No write can run between the generation query and this read, so
            // LOAD_RECORDS reads the same generation.
            let records = sql.exec(LOAD_RECORDS, None)?.to_array::<Record>()?;
            // Corrupt new-format records fail closed; never resurrect stale legacy data.
            let workspace = decode(records)?;
            *self.cache.borrow_mut() = Some((generation, workspace.clone()));
            return Ok(workspace);
        }
        sql.exec("CREATE TABLE IF NOT EXISTS workspace (id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL)",None)?;
        #[derive(Deserialize)]
        struct Legacy {
            body: String,
        }
        let rows = sql
            .exec("SELECT body FROM workspace WHERE id=1", None)?
            .to_array::<Legacy>()?;
        rows.first()
            .map(|row| Workspace::from_json(&row.body).map_err(internal))
            .unwrap_or_else(|| Ok(Workspace::default()))
    }
    pub(crate) async fn save(&self, workspace: &mut Workspace) -> Result<()> {
        self.save_with_sql(workspace, |_| Ok(())).await
    }
    pub(crate) async fn save_with_sql(
        &self,
        workspace: &mut Workspace,
        stage: impl FnOnce(&SqlStorage) -> Result<()> + 'static,
    ) -> Result<()> {
        // All validation and retention precedes the atomic commit. No callback I/O.
        let mut candidate = workspace.clone();
        // Retention precedes lifecycle cleanup, so pruned runs cannot retain scopes.
        candidate.checkpoint().map_err(internal)?;
        *workspace = self.sql_transaction(move |sql| {
            stage(&sql)?;
            write_workspace_records(&sql, &mut candidate)?;
            Ok(candidate)
        })?;
        // Arm recovery before any effect is attempted. A failed transaction must
        // leave a durable wake-up, including when this save runs in an alarm.
        self.arm_workspace_alarm(workspace).await?;
        self.flush_data_tasks(workspace)?;
        self.arm_workspace_alarm(workspace).await
    }
    async fn arm_workspace_alarm(&self, workspace: &Workspace) -> Result<()> {
        if let Some(at) = workspace.next_alarm() {
            self.state
                .storage()
                .set_alarm(std::time::Duration::from_millis(
                    at.saturating_sub(crate::now()),
                ))
                .await?;
        } else {
            self.state.storage().delete_alarm().await?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn row(kind: &str, body: Value) -> Record {
        Record {
            kind: kind.into(),
            body: body.to_string(),
        }
    }
    #[test]
    fn metadata_and_collections_roundtrip_without_exposing_management() {
        let ws = decode(vec![
            row("audit", json!({"id":7,"action":"workflow.saved"})),
            row(
                "metadata",
                json!({"storageVersion":1,"management":{"private":"stored"},"auditSequence":7}),
            ),
        ])
        .unwrap();
        assert_eq!(ws.audit.len(), 1);
        assert_eq!(ws.audit_sequence, 7);
        assert_eq!(ws.management["private"], "stored");
        assert!(ws.snapshot().unwrap().get("management").is_none());
        assert!(ws.snapshot().unwrap().get("audit").is_none());
    }
    #[test]
    fn corrupt_generations_are_not_accepted_as_empty_workspaces() {
        assert!(decode(vec![]).is_err());
        assert!(decode(vec![row("metadata", json!({"storageVersion":2}))]).is_err());
        assert!(
            decode(vec![
                row("metadata", json!({"storageVersion":1})),
                row("unknown", json!({}))
            ])
            .is_err()
        );
        assert!(
            decode(vec![
                row("metadata", json!({"storageVersion":1})),
                row("metadata", json!({"storageVersion":1}))
            ])
            .is_err()
        );
    }
}
