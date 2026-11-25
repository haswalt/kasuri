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
use workflow_service::store::sql::{SqlExec, SqlStore, SqlValue, legacy};
use workflow_service::store::{Collection, StoreError, Tracked};
use workflow_service::{RunRecord, Workspace};

/// Where runs execute (`RUN_EXECUTION`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunExecution {
    Workspace,
    Objects,
    Queues,
}
pub(crate) fn parse_run_execution(value: &str) -> std::result::Result<RunExecution, String> {
    match value.trim() {
        "workspace" => Ok(RunExecution::Workspace),
        "" | "objects" => Ok(RunExecution::Objects),
        "queues" => Ok(RunExecution::Queues),
        other => Err(format!(
            "RUN_EXECUTION must be 'workspace', 'objects' or 'queues', not '{other}'"
        )),
    }
}
/// How long a run object has to report after its start before the start is
/// retried.
const REMOTE_START_RETRY_MS: u64 = 10_000;
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
pub(crate) fn store_error(error: StoreError) -> Error {
    internal(format!("Workspace storage: {error}"))
}
pub(crate) fn tables(sql: &SqlStorage) -> SqlStore<DoSql> {
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
/// Work for run objects that a commit produced: runs to start and control
/// requests to forward, once the transaction has committed.
#[derive(Default)]
pub(crate) struct Handoff {
    /// Run trees to start, each newest run first (root last).
    pub(crate) start: Vec<Vec<RunRecord>>,
    pub(crate) control: Vec<(String, String)>,
}
/// Returns what the caller must hand to run objects after the commit.
pub(crate) fn write_workspace(
    sql: &SqlStorage,
    cache: &WorkspaceCache,
    workspace: &mut Workspace,
) -> Result<Handoff> {
    let mut store = tables(sql);
    prepare(&mut store, cache)?;
    let mut tracked = take_committed(&store, cache)?;
    // Move the candidate in rather than copying it; on failure the transaction
    // rolls back and the caller discards its candidate.
    tracked.workspace = std::mem::take(workspace);
    let written = tracked.commit(&mut store).map_err(store_error)?;
    // Written in list order (newest first), which is each tree's own order.
    let mut offloaded: Vec<(String, Vec<RunRecord>)> = Vec::new();
    for record in written.upserts.iter().filter(|record| record.remote) {
        let record: RunRecord = serde_json::from_str(&record.body).map_err(internal)?;
        let root = match &record.parent {
            Some(parent) => parent.root_id.clone(),
            None => record.id_string().map_err(internal)?,
        };
        match offloaded.iter_mut().find(|(id, _)| *id == root) {
            Some((_, tree)) => tree.push(record),
            None => offloaded.push((root, vec![record])),
        }
    }
    let offloaded: Vec<Vec<RunRecord>> = offloaded.into_iter().map(|(_, tree)| tree).collect();
    // Hand back the workspace with retention applied: the one copy a save makes,
    // since the cache keeps the committed state as well.
    *workspace = tracked.workspace.clone();
    let remote = store.remote_active_ids().map_err(store_error)?;
    crate::data::sync_run_data(sql, workspace, &remote, crate::now())?;
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
    // Never cached or handed back: forwarded exactly once, by this caller.
    let control = std::mem::take(&mut tracked.workspace.remote_controls);
    workspace.remote_controls.clear();
    remember(&store, cache, tracked)?;
    Ok(Handoff {
        start: offloaded,
        control,
    })
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
        workspace.offload = self.run_execution()? != RunExecution::Workspace;
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
    /// `RUN_EXECUTION`: unset, empty or `objects` uses run objects; `workspace`
    /// keeps execution in the workspace, and `queues` steps run objects through
    /// the queue. Only eligible runs leave the workspace.
    pub(crate) fn run_execution(&self) -> Result<RunExecution> {
        let value = self
            .env
            .var("RUN_EXECUTION")
            .map(|v| v.to_string())
            .unwrap_or_default();
        parse_run_execution(&value).map_err(internal)
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
        // Runs already waiting leave in this commit.
        workspace.offload_waiting().map_err(internal)?;
        self.commit_and_hand_off(workspace, stage).await?;
        // Arm recovery before any effect is attempted. A failed transaction must
        // leave a durable wake-up, including when this save runs in an alarm.
        self.arm_workspace_alarm(workspace).await?;
        self.flush_data_tasks(workspace)?;
        // Runs now waiting (their data tasks done) move to run objects.
        if workspace.offload_waiting().map_err(internal)? {
            self.commit_and_hand_off(workspace, |_| Ok(())).await?;
        }
        self.arm_workspace_alarm(workspace).await
    }
    /// Commit the workspace, then hand the run trees it moved out to their run
    /// objects and forward control requests for remote runs.
    async fn commit_and_hand_off(
        &self,
        workspace: &mut Workspace,
        stage: impl FnOnce(&SqlStorage) -> Result<()> + 'static,
    ) -> Result<()> {
        let mut candidate = workspace.clone();
        let cache = self.cache.clone();
        let handoff;
        (*workspace, handoff) = self.sql_transaction(move |sql| {
            stage(&sql)?;
            let handoff = write_workspace(&sql, &cache, &mut candidate)?;
            Ok((candidate, handoff))
        })?;
        // Their index rows are committed; now hand them to their run objects. A
        // failed start is retried from the index (see `restart_remote_runs`).
        for tree in handoff.start {
            if let Err(error) = self.start_remote(tree).await {
                console_error!("Starting a run object failed; it will be retried: {error}");
            }
        }
        for (run, action) in handoff.control {
            if let Err(error) = self.control_remote(&run, &action).await {
                console_error!("Forwarding {action} to run {run} failed: {error}");
            }
        }
        Ok(())
    }
    /// Hand a run tree (newest run first, root last) to its run object, which
    /// creates it if it isn't there yet.
    pub(crate) async fn start_remote(&self, runs: Vec<RunRecord>) -> Result<()> {
        let id = runs
            .last()
            .ok_or_else(|| internal("empty run tree"))?
            .id_string()
            .map_err(internal)?;
        let workspace = self.state.id().to_string();
        let namespace = self.env.durable_object("RUNS")?;
        let stub = namespace
            .id_from_name(&format!("{workspace}/{id}"))?
            .get_stub()?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post).with_body(Some(
            serde_json::to_string(&serde_json::json!({"workspace": workspace, "runs": runs}))?
                .into(),
        ));
        let response = stub
            .fetch_with_request(Request::new_with_init("https://run/start", &init)?)
            .await?;
        if response.status_code() != 200 {
            return Err(internal(format!(
                "run object refused start: {}",
                response.status_code()
            )));
        }
        Ok(())
    }
    /// Retry starting runs whose run object never reported: the start after their
    /// commit may not have arrived. Starting is idempotent.
    pub(crate) async fn restart_remote_runs(&self) -> Result<()> {
        let before = crate::now().saturating_sub(REMOTE_START_RETRY_MS);
        let unstarted = tables(&self.state.storage().sql())
            .unstarted_remote_runs(before)
            .map_err(store_error)?;
        for tree in unstarted {
            if let Err(error) = self.start_remote(tree).await {
                console_error!("Retrying a run object start failed: {error}");
            }
        }
        Ok(())
    }
    /// Forward a control request for a remote run to its tree's run object, if
    /// `id` is a remote run that is still active. Returns its response.
    pub(crate) async fn control_remote(&self, id: &str, action: &str) -> Result<Option<Response>> {
        let Some(root) = tables(&self.state.storage().sql())
            .remote_active_root(id)
            .map_err(store_error)?
        else {
            return Ok(None);
        };
        let workspace = self.state.id().to_string();
        let stub = self
            .env
            .durable_object("RUNS")?
            .id_from_name(&format!("{workspace}/{root}"))?
            .get_stub()?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post).with_body(Some(
            serde_json::json!({"action": action, "run": id})
                .to_string()
                .into(),
        ));
        Ok(Some(
            stub.fetch_with_request(Request::new_with_init("https://run/control", &init)?)
                .await?,
        ))
    }
    /// Apply a run object's report to its tree's index rows, in one transaction.
    /// Older reports are ignored, so reports may arrive late or twice.
    pub(crate) fn apply_run_report(
        &self,
        root: &str,
        runs: Vec<(String, u64, String)>,
    ) -> Result<()> {
        let runs = runs
            .into_iter()
            .map(|(run, version, body)| {
                let record: RunRecord = serde_json::from_str(&body).map_err(internal)?;
                let columns = workflow_service::store::RunColumns::of(&record).map_err(internal)?;
                Ok((run, version, body, columns))
            })
            .collect::<Result<Vec<_>>>()?;
        let root = root.to_owned();
        self.sql_transaction(move |sql| {
            let mut store = tables(&sql);
            for (run, version, body, columns) in &runs {
                workflow_service::store::Store::apply_report(
                    &mut store, &root, run, *version, body, columns,
                )
                .map_err(store_error)?;
            }
            Ok(())
        })
    }
    async fn arm_workspace_alarm(&self, workspace: &Workspace) -> Result<()> {
        // Runs whose run object hasn't reported yet are checked again shortly.
        let unstarted = tables(&self.state.storage().sql())
            .has_unstarted_remote_runs(crate::now().saturating_add(1))
            .map_err(store_error)?;
        let retry = unstarted.then(|| crate::now() + REMOTE_START_RETRY_MS);
        let next = match (workspace.next_alarm(), retry) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        if let Some(at) = next {
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
mod execution_mode_tests {
    use super::{RunExecution, parse_run_execution};
    #[test]
    fn default_is_objects_and_alternatives_remain_explicit() {
        for value in ["", " ", "objects", " objects "] {
            assert_eq!(parse_run_execution(value), Ok(RunExecution::Objects));
        }
        assert_eq!(
            parse_run_execution("workspace"),
            Ok(RunExecution::Workspace)
        );
        assert_eq!(parse_run_execution("queues"), Ok(RunExecution::Queues));
        assert!(parse_run_execution("object").is_err());
    }
}
