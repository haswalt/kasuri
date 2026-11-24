//! Workspace persistence, in one of two layouts chosen by `WORKSPACE_STORAGE`:
//!
//! - **generations** (the default): the whole workspace is written as a new record
//!   generation on every save. One INSERT publishes a complete generation
//!   atomically; readers never combine rows from different saves. Legacy
//!   single-row checkpoints migrate on save.
//! - **tables**: one row per workflow, revision, run, audit entry and webhook, with
//!   indexed run columns (`workflow_service::store::sql`). A save writes only the
//!   rows that changed.
//!
//! Loading a workspace stored in the other layout migrates it in one transaction,
//! in either direction. If a migration fails, the transaction rolls back and the
//! workspace keeps its current layout.
use crate::{WorkflowWorkspace, internal};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
use worker::wasm_bindgen::{JsCast, JsValue, closure::Closure};
use worker::*;
use workflow_service::Workspace;
use workflow_service::store::sql::{SqlExec, SqlStore, SqlValue};
use workflow_service::store::{Collection, StoreError, Tracked};

/// The committed workspace this object last read or wrote, with the marker that
/// identifies the commit it came from. Reused while that commit is still current.
pub(crate) enum Committed {
    /// A decoded record generation.
    Generation(f64, Workspace),
    /// A workspace loaded from (or committed to) the tables, keyed by commit token.
    Tables(String, Tracked),
}
pub(crate) type WorkspaceCache = Rc<RefCell<Option<Committed>>>;

/// Where workspaces are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layout {
    Generations,
    Tables,
}
/// `WORKSPACE_STORAGE`: unset, empty or `generations` keeps whole-workspace
/// generations; `tables` stores one row per entity. Anything else is an error,
/// rather than a silent fallback to a different layout.
pub(crate) fn parse_layout(value: &str) -> std::result::Result<Layout, String> {
    match value.trim() {
        "" | "generations" => Ok(Layout::Generations),
        "tables" => Ok(Layout::Tables),
        other => Err(format!(
            "WORKSPACE_STORAGE must be 'generations' or 'tables', not '{other}'"
        )),
    }
}

/// Durable Object SQLite as a [`SqlExec`].
pub(crate) struct DoSql(pub(crate) SqlStorage);
impl SqlExec for DoSql {
    fn query(
        &self,
        query: &str,
        params: Vec<SqlValue>,
    ) -> std::result::Result<Vec<Vec<SqlValue>>, StoreError> {
        let bindings = params
            .into_iter()
            .map(|value| match value {
                SqlValue::Null => SqlStorageValue::Null,
                SqlValue::Integer(i) => SqlStorageValue::Integer(i),
                SqlValue::Real(f) => SqlStorageValue::Float(f),
                SqlValue::Text(t) => SqlStorageValue::String(t),
            })
            .collect::<Vec<_>>();
        let cursor = self
            .0
            .exec(query, bindings)
            .map_err(|e| StoreError::Invalid(e.to_string()))?;
        cursor
            .raw()
            .map(|row| {
                let row = row.map_err(|e| StoreError::Invalid(e.to_string()))?;
                Ok(row
                    .into_iter()
                    .map(|value| match value {
                        SqlStorageValue::Null => SqlValue::Null,
                        SqlStorageValue::Boolean(b) => SqlValue::Integer(b.into()),
                        SqlStorageValue::Integer(i) => SqlValue::Integer(i),
                        SqlStorageValue::Float(f) => SqlValue::Real(f),
                        SqlStorageValue::String(t) => SqlValue::Text(t),
                        SqlStorageValue::Blob(b) => {
                            SqlValue::Text(String::from_utf8_lossy(&b).into_owned())
                        }
                    })
                    .collect())
            })
            .collect()
    }
}
fn store_error(error: StoreError) -> Error {
    internal(format!("Workspace storage: {error}"))
}
fn tables(sql: &SqlStorage) -> SqlStore<DoSql> {
    SqlStore::new(DoSql(sql.clone()))
}
/// A marker for one commit. Random, so a rolled-back commit's marker never
/// matches a later one.
fn commit_token() -> String {
    format!(
        "{:x}-{:x}",
        crate::now(),
        (js_sys::Math::random() * 9_007_199_254_740_992.0) as u64
    )
}
const CREATE_LEGACY: &str =
    "CREATE TABLE IF NOT EXISTS workspace (id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL)";

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
    sql.exec(CREATE_LEGACY, None)?;
    sql.exec("DELETE FROM workspace WHERE id=1", None)?;
    Ok(())
}
/// Commit `workspace` in whichever layout the object currently uses. Must run
/// inside `sql_transaction`. Retention is applied to `workspace` first.
pub(crate) fn write_workspace(
    sql: &SqlStorage,
    cache: &WorkspaceCache,
    workspace: &mut Workspace,
) -> Result<()> {
    let store = tables(sql);
    if store.format().map_err(store_error)?.is_none() {
        return write_generation(sql, workspace);
    }
    let mut tracked = baseline(&store, cache)?;
    tracked.workspace = workspace.clone();
    let mut store = store;
    let written = tracked.commit(&mut store).map_err(store_error)?;
    // Hand back the workspace with retention applied, as a generation save does.
    workspace.clone_from(&tracked.workspace);
    crate::data::sync_run_data(sql, workspace, crate::now())?;
    let affects_webhooks = written
        .upserts
        .iter()
        .map(|record| &record.key)
        .chain(&written.deletes)
        .any(|key| {
            matches!(
                key.collection,
                Collection::Workflow | Collection::Revision | Collection::Webhook
            )
        });
    if affects_webhooks {
        write_webhook_auth(sql, workspace)?;
    }
    let token = commit_token();
    store.set_commit(&token).map_err(store_error)?;
    // If the surrounding transaction rolls back, the stored token stays the old
    // one, so this entry is never mistaken for the committed workspace.
    *cache.borrow_mut() = Some(Committed::Tables(token, tracked));
    Ok(())
}
/// The committed workspace as stored in the tables: the cached copy when it is
/// still current, otherwise a fresh load.
fn baseline(store: &SqlStore<DoSql>, cache: &WorkspaceCache) -> Result<Tracked> {
    let token = store.commit().map_err(store_error)?;
    if let (Some(token), Some(Committed::Tables(cached, tracked))) = (&token, &*cache.borrow())
        && cached == token
    {
        return Ok(tracked.clone());
    }
    let tracked = Tracked::load(store).map_err(store_error)?;
    if let Some(token) = token {
        *cache.borrow_mut() = Some(Committed::Tables(token, tracked.clone()));
    }
    Ok(tracked)
}
fn write_generation(sql: &SqlStorage, workspace: &mut Workspace) -> Result<()> {
    let body = workspace.checkpoint().map_err(internal)?;
    crate::data::sync_run_data(sql, workspace, crate::now())?;
    write_encoded_records(sql, body)?;
    write_webhook_auth(sql, workspace)
}
/// Read the workspace from record generations, or the legacy single row.
fn read_generations(sql: &SqlStorage, cache: Option<&WorkspaceCache>) -> Result<Workspace> {
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
        if let Some(cache) = cache
            && let Some(Committed::Generation(cached, workspace)) = &*cache.borrow()
            && *cached == generation
        {
            return Ok(workspace.clone());
        }
        // No write can run between the generation query and this read, so
        // LOAD_RECORDS reads the same generation.
        let records = sql.exec(LOAD_RECORDS, None)?.to_array::<Record>()?;
        // Corrupt new-format records fail closed; never resurrect stale legacy data.
        let workspace = decode(records)?;
        if let Some(cache) = cache {
            *cache.borrow_mut() = Some(Committed::Generation(generation, workspace.clone()));
        }
        return Ok(workspace);
    }
    sql.exec(CREATE_LEGACY, None)?;
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
/// Move a workspace from generations (or the legacy row) into the tables.
fn migrate_to_tables(sql: &SqlStorage, cache: &WorkspaceCache) -> Result<Workspace> {
    let workspace = read_generations(sql, None)?;
    let mut store = tables(sql);
    // Leftovers of an earlier migration back to generations are discarded.
    store.drop_tables().map_err(store_error)?;
    store.create().map_err(store_error)?;
    let mut tracked = Tracked::load(&store).map_err(store_error)?;
    tracked.workspace = workspace;
    tracked.commit(&mut store).map_err(store_error)?;
    store.set_format().map_err(store_error)?;
    write_webhook_auth(sql, &tracked.workspace)?;
    sql.exec("DROP TABLE IF EXISTS workspace_records", None)?;
    sql.exec("DROP TABLE IF EXISTS workspace", None)?;
    let token = commit_token();
    store.set_commit(&token).map_err(store_error)?;
    let workspace = tracked.workspace.clone();
    *cache.borrow_mut() = Some(Committed::Tables(token, tracked));
    Ok(workspace)
}
/// Move a workspace from the tables back to record generations.
fn migrate_to_generations(sql: &SqlStorage, cache: &WorkspaceCache) -> Result<Workspace> {
    let store = tables(sql);
    let mut workspace = Tracked::load(&store).map_err(store_error)?.workspace;
    store.drop_tables().map_err(store_error)?;
    write_generation(sql, &mut workspace)?;
    *cache.borrow_mut() = None;
    Ok(workspace)
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
    fn layout(&self) -> Result<Layout> {
        let value = self
            .env
            .var("WORKSPACE_STORAGE")
            .map(|v| v.to_string())
            .unwrap_or_default();
        parse_layout(&value).map_err(internal)
    }
    fn load_stored(&self) -> Result<Workspace> {
        let wanted = self.layout()?;
        let sql = self.state.storage().sql();
        let in_tables = tables(&sql).format().map_err(store_error)?.is_some();
        match (wanted, in_tables) {
            (Layout::Generations, false) => read_generations(&sql, Some(&self.cache)),
            (Layout::Tables, true) => Ok(baseline(&tables(&sql), &self.cache)?.workspace),
            (Layout::Tables, false) => {
                let cache = self.cache.clone();
                self.sql_transaction(move |sql| migrate_to_tables(&sql, &cache))
                    .or_else(|error| {
                        console_error!("Workspace migration to tables failed: {error}");
                        read_generations(&sql, Some(&self.cache))
                    })
            }
            (Layout::Generations, true) => {
                let cache = self.cache.clone();
                self.sql_transaction(move |sql| migrate_to_generations(&sql, &cache))
                    .or_else(|error| {
                        console_error!("Workspace migration to generations failed: {error}");
                        Ok(baseline(&tables(&sql), &self.cache)?.workspace)
                    })
            }
        }
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
        let cache = self.cache.clone();
        *workspace = self.sql_transaction(move |sql| {
            stage(&sql)?;
            write_workspace(&sql, &cache, &mut candidate)?;
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
    fn storage_layout_is_explicit() {
        assert_eq!(parse_layout(""), Ok(Layout::Generations));
        assert_eq!(parse_layout(" generations "), Ok(Layout::Generations));
        assert_eq!(parse_layout("tables"), Ok(Layout::Tables));
        assert!(parse_layout("table").is_err());
        assert!(parse_layout("Tables").is_err());
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
