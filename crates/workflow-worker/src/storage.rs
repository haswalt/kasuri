//! Workspace persistence: one SQLite row per workflow, revision, run, audit entry
//! and webhook, with indexed run columns (`workflow_service::store::sql`). A save
//! writes only the rows that changed, in one transaction.
//!
//! Workspaces saved by earlier releases (record generations, or the single-row
//! layout before them) migrate into the tables on first load, in one transaction.
//! If the migration fails, it rolls back, leaving the old layout intact, and the
//! next load tries again.
use crate::{WorkflowWorkspace, internal};
use std::{cell::RefCell, rc::Rc};
use worker::wasm_bindgen::{JsCast, JsValue, closure::Closure};
use worker::*;
use workflow_service::Workspace;
use workflow_service::store::sql::{SqlExec, SqlStore, SqlValue, legacy};
use workflow_service::store::{Collection, StoreError, Tracked};

/// The committed workspace this object last read or wrote, with the commit token
/// it came from. Reused while that token is still the stored one, so most
/// requests don't decode the workspace.
pub(crate) type WorkspaceCache = Rc<RefCell<Option<(String, Tracked)>>>;

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
/// Commit `workspace`, writing only the rows that changed. Must run inside
/// `sql_transaction`. Retention is applied to `workspace` first.
pub(crate) fn write_workspace(
    sql: &SqlStorage,
    cache: &WorkspaceCache,
    workspace: &mut Workspace,
) -> Result<()> {
    let mut store = tables(sql);
    prepare(&mut store, cache)?;
    let mut tracked = take_committed(&store, cache)?;
    // Move the candidate in rather than copying it; on failure the transaction
    // rolls back and the caller discards its candidate.
    tracked.workspace = std::mem::take(workspace);
    let written = tracked.commit(&mut store).map_err(store_error)?;
    // Hand back the workspace with retention applied: the one copy a save makes,
    // since the cache keeps the committed state as well.
    *workspace = tracked.workspace.clone();
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
    remember(&store, cache, tracked)
}
/// Record a new commit token and cache the committed workspace under it. If the
/// surrounding transaction rolls back, the stored token stays the old one, so the
/// cached copy is never mistaken for the committed workspace.
fn remember(store: &SqlStore<DoSql>, cache: &WorkspaceCache, tracked: Tracked) -> Result<()> {
    let token = commit_token();
    store.set_commit(&token).map_err(store_error)?;
    *cache.borrow_mut() = Some((token, tracked));
    Ok(())
}
/// Bring the tables to the current layout: migrate a workspace from an earlier
/// release (or start a new one), or upgrade tables from an older version. Must run
/// inside `sql_transaction` unless the tables are already current.
fn prepare(store: &mut SqlStore<DoSql>, cache: &WorkspaceCache) -> Result<()> {
    if store.format().map_err(store_error)?.is_none() {
        let tracked = legacy::migrate(store).map_err(store_error)?;
        write_webhook_auth(&store.sql().0, &tracked.workspace)?;
        return remember(store, cache, tracked);
    }
    store.upgrade().map_err(store_error)?;
    Ok(())
}
/// The committed state for a write. When the cache holds it, it is taken out
/// rather than copied: the write replaces it, and if the write fails the cache is
/// simply empty and the next read loads from storage.
fn take_committed(store: &SqlStore<DoSql>, cache: &WorkspaceCache) -> Result<Tracked> {
    let token = store.commit().map_err(store_error)?;
    let mut cached = cache.borrow_mut();
    let current = matches!((&token, &*cached), (Some(token), Some((at, _))) if at == token);
    if current && let Some((_, tracked)) = cached.take() {
        return Ok(tracked);
    }
    *cached = None;
    Tracked::load(store).map_err(store_error)
}
/// The committed workspace for a read: a copy of the cached workspace when it is
/// still current (the change-tracking baseline is not copied), otherwise a fresh
/// load, which is cached.
fn committed_workspace(store: &SqlStore<DoSql>, cache: &WorkspaceCache) -> Result<Workspace> {
    let token = store.commit().map_err(store_error)?;
    if let (Some(token), Some((current, tracked))) = (&token, &*cache.borrow())
        && current == token
    {
        return Ok(tracked.workspace.clone());
    }
    let tracked = Tracked::load(store).map_err(store_error)?;
    let workspace = tracked.workspace.clone();
    if let Some(token) = token {
        *cache.borrow_mut() = Some((token, tracked));
    }
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
        workspace.retention = self.retention()?.at(crate::now());
        // Finished runs are read from storage on demand.
        workspace.archive =
            workflow_service::Archive::new(Rc::new(tables(&self.state.storage().sql())));
        Ok(workspace)
    }
    /// `RUN_HISTORY`, `RUN_HISTORY_DAYS` and `FAILED_RUN_HISTORY_DAYS` from the
    /// environment. An invalid value fails requests rather than silently keeping
    /// a different amount of history.
    fn retention(&self) -> Result<workflow_service::store::RetentionPolicy> {
        let var = |name: &str| {
            self.env
                .var(name)
                .map(|v| v.to_string())
                .unwrap_or_default()
        };
        workflow_service::store::RetentionPolicy::parse(
            &var("RUN_HISTORY"),
            &var("RUN_HISTORY_DAYS"),
            &var("FAILED_RUN_HISTORY_DAYS"),
        )
        .map_err(internal)
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
        let store = tables(&sql);
        if store.current().map_err(store_error)? {
            return committed_workspace(&store, &self.cache);
        }
        // Migrate a workspace saved by an earlier release (or start a new one), or
        // upgrade tables from an older layout version.
        let cache = self.cache.clone();
        self.sql_transaction(move |sql| {
            let mut store = tables(&sql);
            prepare(&mut store, &cache)?;
            committed_workspace(&store, &cache)
        })
        .inspect_err(|error| console_error!("Workspace migration to tables failed: {error}"))
    }
    pub(crate) async fn save(&self, workspace: &mut Workspace) -> Result<()> {
        self.save_with_sql(workspace, |_| Ok(())).await
    }
    pub(crate) async fn save_with_sql(
        &self,
        workspace: &mut Workspace,
        stage: impl FnOnce(&SqlStorage) -> Result<()> + 'static,
    ) -> Result<()> {
        // No callback I/O inside the transaction. Retention runs in the commit,
        // before lifecycle cleanup, so pruned runs cannot retain data scopes.
        let mut candidate = workspace.clone();
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
