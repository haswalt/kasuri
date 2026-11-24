//! Per-entity workspace storage boundary (stage 1, phase 1 of
//! `docs/workspace-storage.md`).
//!
//! A [`Workspace`] is split into keyed entities: workflows, revisions, runs, audit
//! entries and webhook registrations, plus one metadata entity holding everything
//! else. A [`Store`] holds those records. [`Tracked`] loads a workspace from a
//! store and, after an operation, commits only the entities that changed.
//!
//! The service still operates on a whole `Workspace`. [`MemoryStore`] keeps
//! records in memory for tests, and [`sql::SqlStore`] keeps them in SQLite tables
//! for hosts. Later phases load entities on demand.
use crate::{ApiError, RunRecord, Workspace};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use workflow_engine::RunStatus;

#[path = "store_sql.rs"]
pub mod sql;

/// The workspace collections stored as individual records, in document order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Collection {
    Workflow,
    Revision,
    Run,
    Audit,
    Webhook,
}
impl Collection {
    pub const ALL: [Collection; 5] = [
        Collection::Workflow,
        Collection::Revision,
        Collection::Run,
        Collection::Audit,
        Collection::Webhook,
    ];
    /// The `Workspace` field (as serialized) that holds this collection.
    pub fn field(self) -> &'static str {
        match self {
            Collection::Workflow => "workflows",
            Collection::Revision => "revisions",
            Collection::Run => "runs",
            Collection::Audit => "audit",
            Collection::Webhook => "webhooks",
        }
    }
}

/// Identity of one stored entity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityKey {
    pub collection: Collection,
    pub id: String,
}

/// One stored entity. `order` reproduces the entity's position in its collection:
/// records sort by ascending order. `body` is the entity's JSON encoding.
/// Run records also carry [`RunColumns`], derived from the body when written, so
/// run queries never decode run bodies. Stores must be given columns with every
/// run they write, but need not return them when reading records back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub key: EntityKey,
    pub order: i64,
    pub body: String,
    pub columns: Option<RunColumns>,
    /// A finished run that nothing in memory depends on any more. It stays stored
    /// (for run lists, details and history) but is not loaded with the workspace.
    pub archived: bool,
}

/// Indexed facts about a run, derived from its [`RunRecord`] whenever the run is
/// written. They answer every [`RunQueries`] question without decoding the run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunColumns {
    /// Engine status (`running`, `paused`, `succeeded`, `failed`, `cancelled`).
    pub status: String,
    /// The workflow the run belongs to.
    pub workflow_id: String,
    pub created_at: u64,
    /// When the run next needs the engine (a wait, retry or deadline).
    pub wake_at: Option<u64>,
    /// Whether the run holds an open webhook response window.
    pub response_open: bool,
    /// The open window's deadline, if it has one.
    pub response_deadline: Option<u64>,
    /// The invoking parent run and the root of its invocation tree.
    pub parent_id: Option<String>,
    pub root_id: Option<String>,
    /// Child runs this run awaits and has not yet collected.
    pub awaits: Vec<String>,
    /// The run's list entry (`RunRecord::view(true)`), as JSON.
    pub summary: String,
}
impl RunColumns {
    pub fn of(record: &RunRecord) -> Result<Self, ApiError> {
        let window = record.webhook_response.as_ref().filter(|w| w.open());
        let summary = record.view(true)?;
        Ok(Self {
            status: status_text(record.status()?),
            workflow_id: summary["workflowId"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            created_at: record.created_at,
            wake_at: record.wake_at,
            response_open: window.is_some(),
            response_deadline: window.and_then(|w| w.deadline_ms),
            parent_id: record.parent.as_ref().map(|p| p.run_id.clone()),
            root_id: record.parent.as_ref().map(|p| p.root_id.clone()),
            awaits: record
                .children
                .iter()
                .filter(|c| c.awaited && !c.collected)
                .map(|c| c.run_id.clone())
                .collect(),
            summary: summary.to_string(),
        })
    }
    fn active(&self) -> bool {
        matches!(self.status.as_str(), "running" | "paused")
    }
}
fn status_text(status: RunStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The run questions the service asks. `Workspace` answers them from runs in
/// memory; stores answer them from [`RunColumns`] without loading runs.
pub trait RunQueries {
    type Error;
    /// The earliest time any run needs the engine: a wake time, or the deadline of
    /// an open webhook response window.
    fn next_run_wake(&self) -> Result<Option<u64>, Self::Error>;
    /// Runs the alarm must visit at `now`, in list order: those due to wake, and
    /// those holding an open webhook response window (which may need settling).
    fn due_runs(&self, now: u64) -> Result<Vec<String>, Self::Error>;
    /// Running and paused runs, plus (when asked) runs holding an open webhook
    /// response window.
    fn active_runs(&self, count_open_responses: bool) -> Result<usize, Self::Error>;
    /// List entries (`RunRecord::view(true)`) for the newest `limit` runs.
    fn run_summaries(&self, limit: usize) -> Result<Vec<Value>, Self::Error>;
    /// Whether an active run depends on `id`, as its parent, its invocation root or
    /// an awaited, uncollected child. Protected runs are never pruned.
    fn run_protected(&self, id: &str) -> Result<bool, Self::Error>;
}

/// The rows one operation changed. Applying it must be atomic.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeSet {
    /// The new metadata entity, when it changed.
    pub meta: Option<String>,
    pub upserts: Vec<Record>,
    pub deletes: Vec<EntityKey>,
}
impl ChangeSet {
    pub fn is_empty(&self) -> bool {
        self.meta.is_none() && self.upserts.is_empty() && self.deletes.is_empty()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum StoreError {
    /// Two entities in one collection share a key.
    DuplicateKey(EntityKey),
    /// An entity has no usable identity (for example an audit entry without an id).
    MissingKey(Collection),
    /// Stored or in-memory data could not be encoded or decoded.
    Invalid(String),
}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::DuplicateKey(key) => {
                write!(f, "duplicate {:?} key {}", key.collection, key.id)
            }
            StoreError::MissingKey(collection) => write!(f, "{collection:?} entity has no key"),
            StoreError::Invalid(message) => f.write_str(message),
        }
    }
}

/// Durable per-entity storage for one workspace.
pub trait Store {
    /// The metadata entity, or `None` for a workspace that has never been saved.
    fn meta(&self) -> Result<Option<String>, StoreError>;
    /// Every record of a collection that is loaded with the workspace, sorted by
    /// ascending `order`. Archived runs are not included.
    fn records(&self, collection: Collection) -> Result<Vec<Record>, StoreError>;
    /// One record by key, archived or not.
    fn record(&self, key: &EntityKey) -> Result<Option<Record>, StoreError>;
    /// The lowest order key in a collection, archived records included.
    fn min_order(&self, collection: Collection) -> Result<Option<i64>, StoreError>;
    /// Apply a change set atomically: all of it or none of it.
    fn apply(&mut self, changes: &ChangeSet) -> Result<(), StoreError>;
    /// Delete archived runs that `retention` no longer keeps: beyond its count in
    /// each of `workflows`, or older than its cutoffs in any workflow. Runs held in
    /// memory are never deleted. Part of a commit: atomic with `apply`.
    fn prune_runs(
        &mut self,
        retention: &RunRetention,
        workflows: &[String],
    ) -> Result<(), StoreError>;
}

/// Read access to every stored run, including archived ones the workspace no
/// longer holds in memory. Hosts attach one to the workspace they load.
pub trait RunArchive {
    /// A stored run by ID, archived or not, as last committed.
    fn stored_run(&self, id: &str) -> Result<Option<RunRecord>, StoreError>;
    /// List entries (`RunRecord::view(true)`) for the newest `limit` stored runs,
    /// optionally of one workflow and status, newest first.
    fn list_runs(
        &self,
        limit: usize,
        workflow_id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<Value>, StoreError>;
}

/// An in-memory [`Store`] for tests and hosts without durable storage.
#[derive(Clone, Debug, Default)]
pub struct MemoryStore {
    meta: Option<String>,
    records: BTreeMap<EntityKey, Stored>,
}
#[derive(Clone, Debug)]
struct Stored {
    order: i64,
    body: String,
    columns: Option<RunColumns>,
    archived: bool,
}
impl Stored {
    fn record(&self, key: &EntityKey) -> Record {
        Record {
            key: key.clone(),
            order: self.order,
            body: self.body.clone(),
            columns: self.columns.clone(),
            archived: self.archived,
        }
    }
}
impl MemoryStore {
    fn run_columns(&self) -> impl Iterator<Item = (&EntityKey, i64, &RunColumns)> {
        let mut runs: Vec<_> = self
            .records
            .iter()
            .filter(|(key, _)| key.collection == Collection::Run)
            .filter_map(|(key, row)| row.columns.as_ref().map(|c| (key, row.order, c)))
            .collect();
        runs.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)));
        runs.into_iter()
    }
    /// Number of stored entity records (excluding metadata).
    pub fn len(&self) -> usize {
        self.records.len()
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty() && self.meta.is_none()
    }
}
impl Store for MemoryStore {
    fn meta(&self) -> Result<Option<String>, StoreError> {
        Ok(self.meta.clone())
    }
    fn records(&self, collection: Collection) -> Result<Vec<Record>, StoreError> {
        let mut records: Vec<Record> = self
            .records
            .iter()
            .filter(|(key, row)| key.collection == collection && !row.archived)
            .map(|(key, row)| row.record(key))
            .collect();
        records.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.key.cmp(&b.key)));
        Ok(records)
    }
    fn record(&self, key: &EntityKey) -> Result<Option<Record>, StoreError> {
        Ok(self.records.get(key).map(|row| row.record(key)))
    }
    fn min_order(&self, collection: Collection) -> Result<Option<i64>, StoreError> {
        Ok(self
            .records
            .iter()
            .filter(|(key, _)| key.collection == collection)
            .map(|(_, row)| row.order)
            .min())
    }
    fn prune_runs(
        &mut self,
        retention: &RunRetention,
        workflows: &[String],
    ) -> Result<(), StoreError> {
        let mut doomed: Vec<EntityKey> = Vec::new();
        if let Some(keep) = retention.per_workflow {
            for workflow in workflows {
                doomed.extend(
                    self.run_columns()
                        .filter(|(_, _, c)| &c.workflow_id == workflow)
                        .skip(keep)
                        .map(|(key, _, _)| key.clone()),
                );
            }
        }
        doomed.extend(
            self.run_columns()
                .filter(|(_, _, c)| retention.expired(&c.status, c.created_at))
                .map(|(key, _, _)| key.clone()),
        );
        for key in doomed {
            if self.records.get(&key).is_some_and(|row| row.archived) {
                self.records.remove(&key);
            }
        }
        Ok(())
    }
    fn apply(&mut self, changes: &ChangeSet) -> Result<(), StoreError> {
        // Nothing here can fail part-way, so applying in place is atomic.
        if let Some(meta) = &changes.meta {
            self.meta = Some(meta.clone());
        }
        for key in &changes.deletes {
            self.records.remove(key);
        }
        for record in &changes.upserts {
            self.records.insert(
                record.key.clone(),
                Stored {
                    order: record.order,
                    body: record.body.clone(),
                    columns: record.columns.clone(),
                    archived: record.archived,
                },
            );
        }
        Ok(())
    }
}
impl RunArchive for MemoryStore {
    fn stored_run(&self, id: &str) -> Result<Option<RunRecord>, StoreError> {
        let key = EntityKey {
            collection: Collection::Run,
            id: id.to_owned(),
        };
        self.records
            .get(&key)
            .map(|row| decode_run(&row.body))
            .transpose()
    }
    fn list_runs(
        &self,
        limit: usize,
        workflow_id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<Value>, StoreError> {
        self.run_columns()
            .filter(|(_, _, c)| workflow_id.is_none_or(|w| c.workflow_id == w))
            .filter(|(_, _, c)| status.is_none_or(|s| c.status == s))
            .take(limit)
            .map(|(_, _, c)| {
                serde_json::from_str(&c.summary).map_err(|e| StoreError::Invalid(e.to_string()))
            })
            .collect()
    }
}
/// Decode a stored run body.
pub(crate) fn decode_run(body: &str) -> Result<RunRecord, StoreError> {
    let mut record: RunRecord =
        serde_json::from_str(body).map_err(|e| StoreError::Invalid(format!("run: {e}")))?;
    record.backfill();
    Ok(record)
}

impl RunQueries for MemoryStore {
    type Error = StoreError;
    fn next_run_wake(&self) -> Result<Option<u64>, StoreError> {
        Ok(self
            .run_columns()
            .flat_map(|(_, _, c)| [c.wake_at, c.response_deadline])
            .flatten()
            .min())
    }
    fn due_runs(&self, now: u64) -> Result<Vec<String>, StoreError> {
        Ok(self
            .run_columns()
            .filter(|(_, _, c)| c.response_open || c.wake_at.is_some_and(|at| at <= now))
            .map(|(key, _, _)| key.id.clone())
            .collect())
    }
    fn active_runs(&self, count_open_responses: bool) -> Result<usize, StoreError> {
        Ok(self
            .run_columns()
            .filter(|(_, _, c)| (count_open_responses && c.response_open) || c.active())
            .count())
    }
    fn run_summaries(&self, limit: usize) -> Result<Vec<Value>, StoreError> {
        self.run_columns()
            .take(limit)
            .map(|(_, _, c)| {
                serde_json::from_str(&c.summary).map_err(|e| StoreError::Invalid(e.to_string()))
            })
            .collect()
    }
    fn run_protected(&self, id: &str) -> Result<bool, StoreError> {
        Ok(self.run_columns().any(|(_, _, c)| {
            c.active()
                && (c.parent_id.as_deref() == Some(id)
                    || c.root_id.as_deref() == Some(id)
                    || c.awaits.iter().any(|a| a == id))
        }))
    }
}

/// Today's in-memory answers: the reference behaviour every store must match.
impl RunQueries for Workspace {
    type Error = ApiError;
    fn next_run_wake(&self) -> Result<Option<u64>, ApiError> {
        Ok(self
            .runs
            .iter()
            .filter_map(|r| r.wake_at)
            .chain(self.runs.iter().filter_map(|r| {
                r.webhook_response
                    .as_ref()
                    .filter(|w| w.open())
                    .and_then(|w| w.deadline_ms)
            }))
            .min())
    }
    fn due_runs(&self, now: u64) -> Result<Vec<String>, ApiError> {
        self.runs
            .iter()
            .filter(|r| {
                r.webhook_response.as_ref().is_some_and(|w| w.open())
                    || r.wake_at.is_some_and(|at| at <= now)
            })
            .map(RunRecord::id_string)
            .collect()
    }
    fn active_runs(&self, count_open_responses: bool) -> Result<usize, ApiError> {
        Ok(self
            .runs
            .iter()
            .filter(|r| {
                (count_open_responses && r.webhook_response.as_ref().is_some_and(|w| w.open()))
                    || r.status()
                        .is_ok_and(|s| matches!(s, RunStatus::Running | RunStatus::Paused))
            })
            .count())
    }
    fn run_summaries(&self, limit: usize) -> Result<Vec<Value>, ApiError> {
        self.runs.iter().take(limit).map(|r| r.view(true)).collect()
    }
    fn run_protected(&self, id: &str) -> Result<bool, ApiError> {
        Ok(self.runs.iter().any(|r| {
            r.status()
                .is_ok_and(|s| matches!(s, RunStatus::Running | RunStatus::Paused))
                && (r
                    .parent
                    .as_ref()
                    .is_some_and(|p| p.root_id == id || p.run_id == id)
                    || r.children
                        .iter()
                        .any(|c| c.awaited && !c.collected && c.run_id == id))
        }))
    }
}

/// Gap between consecutive order keys when (re)numbering, leaving room to insert
/// new entities before, after or between existing ones without renumbering.
pub const ORDER_SPACING: i64 = 1 << 20;

/// A workspace loaded from a [`Store`], with the stored state it was loaded from.
#[derive(Debug, Clone)]
pub struct Tracked {
    pub workspace: Workspace,
    meta: Option<String>,
    /// Each stored entity's order and a digest of its body: enough to tell what an
    /// operation changed without keeping a second copy of every body in memory.
    baseline: HashMap<EntityKey, (i64, Digest)>,
    /// The lowest stored run order, archived runs included. New runs (always the
    /// newest) take keys below it.
    run_floor: Option<i64>,
}
impl Tracked {
    /// Load a workspace from its records. An empty store yields an empty workspace.
    pub fn load(store: &impl Store) -> Result<Self, StoreError> {
        let meta = store.meta()?;
        let mut root = match &meta {
            Some(text) => serde_json::from_str::<Value>(text)
                .map_err(|e| StoreError::Invalid(format!("metadata: {e}")))?,
            None => Value::Object(Map::new()),
        };
        let object = root
            .as_object_mut()
            .ok_or_else(|| StoreError::Invalid("metadata is not an object".into()))?;
        let mut baseline = HashMap::new();
        let mut encoded = meta.as_ref().map_or(0, String::len);
        for collection in Collection::ALL {
            let mut items = Vec::new();
            for record in store.records(collection)? {
                encoded += record.body.len();
                items.push(serde_json::from_str::<Value>(&record.body).map_err(|e| {
                    StoreError::Invalid(format!("{:?} {}: {e}", collection, record.key.id))
                })?);
                baseline.insert(record.key, (record.order, digest(&record.body)));
            }
            object.insert(collection.field().into(), Value::Array(items));
        }
        let workspace =
            Workspace::from_value(root, encoded).map_err(|e| StoreError::Invalid(e.message))?;
        Ok(Self {
            workspace,
            meta,
            baseline,
            run_floor: store.min_order(Collection::Run)?,
        })
    }

    /// Archive finished runs, apply retention, then return the rows that differ
    /// from what was loaded. The tracked baseline is not changed.
    ///
    /// A finished run that no run in memory depends on is written with
    /// `archived` set and removed from the workspace: it stays stored for lists and
    /// details, but later loads don't read it. Each entity is encoded once, and
    /// retention works from those encoded sizes, so the whole workspace is never
    /// encoded as one document.
    pub fn changes(&mut self) -> Result<ChangeSet, StoreError> {
        let (mut meta, mut collections) = split(&self.workspace)?;
        let mut changes = ChangeSet::default();
        let mut seen = HashMap::new();
        let invalid = |e: ApiError| StoreError::Invalid(e.message);

        // Runs take their keys over the whole list first, so a run created and
        // archived in this same operation still sorts as the newest.
        let mut run_orders = HashMap::new();
        if let Some((_, entities)) = collections.iter_mut().find(|(c, _)| *c == Collection::Run) {
            let orders = assign_orders(entities, &self.baseline, self.run_floor);
            let archive: Vec<bool> = (0..entities.len())
                .map(|index| self.workspace.run_archivable(index))
                .collect();
            let mut resident = Vec::with_capacity(entities.len());
            for (index, ((key, body), order)) in entities.drain(..).zip(orders).enumerate() {
                if archive[index] {
                    let columns = RunColumns::of(&self.workspace.runs[index]).map_err(invalid)?;
                    seen.insert(key.clone(), ());
                    changes.upserts.push(Record {
                        key,
                        order,
                        body,
                        columns: Some(columns),
                        archived: true,
                    });
                } else {
                    run_orders.insert(key.clone(), order);
                    resident.push((key, body));
                }
            }
            *entities = resident;
            let mut index = 0;
            self.workspace.runs.retain(|_| {
                index += 1;
                !archive[index - 1]
            });
        }

        // Retention over what stays in memory.
        let total = encoded_total(&meta, &collections);
        let sizes = |collection: Collection, collections: &[(Collection, Entities)]| {
            collections
                .iter()
                .find(|(c, _)| *c == collection)
                .map(|(_, entities)| entities.iter().map(|(_, body)| body.len() + 1).collect())
                .unwrap_or_else(Vec::new)
        };
        let (runs, audit) = (self.workspace.runs.len(), self.workspace.audit.len());
        self.workspace
            .retain(
                total,
                &sizes(Collection::Run, &collections),
                &sizes(Collection::Audit, &collections),
            )
            .map_err(invalid)?;
        if self.workspace.runs.len() != runs || self.workspace.audit.len() != audit {
            // Drop the entities retention removed; the rest are already encoded.
            let kept = |collection: Collection| -> Result<HashMap<String, ()>, StoreError> {
                Ok(match collection {
                    Collection::Run => self
                        .workspace
                        .runs
                        .iter()
                        .map(|r| r.id_string().map(|id| (id, ())))
                        .collect::<Result<_, _>>()
                        .map_err(|_| StoreError::MissingKey(Collection::Run))?,
                    _ => self
                        .workspace
                        .audit
                        .iter()
                        .filter_map(|e| e.get("id").and_then(Value::as_u64))
                        .map(|id| (id.to_string(), ()))
                        .collect(),
                })
            };
            for (collection, entities) in &mut collections {
                if matches!(collection, Collection::Run | Collection::Audit) {
                    let kept = kept(*collection)?;
                    entities.retain(|(key, _)| kept.contains_key(&key.id));
                }
            }
            // Retention never changes metadata, but re-derive it to be certain.
            meta = split_meta(&self.workspace)?;
        }
        changes.meta = (self.meta.as_deref() != Some(meta.as_str())).then_some(meta);

        for (collection, entities) in collections {
            let orders: Vec<i64> = match collection {
                Collection::Run => entities.iter().map(|(key, _)| run_orders[key]).collect(),
                _ => assign_orders(&entities, &self.baseline, None),
            };
            for (index, ((key, body), order)) in entities.into_iter().zip(orders).enumerate() {
                let unchanged = self
                    .baseline
                    .get(&key)
                    .is_some_and(|(old_order, old_digest)| {
                        *old_order == order && *old_digest == digest(&body)
                    });
                seen.insert(key.clone(), ());
                if !unchanged {
                    // Run columns are derived only for runs being written.
                    let columns = match collection {
                        Collection::Run => {
                            Some(RunColumns::of(&self.workspace.runs[index]).map_err(invalid)?)
                        }
                        _ => None,
                    };
                    changes.upserts.push(Record {
                        key,
                        order,
                        body,
                        columns,
                        archived: false,
                    });
                }
            }
        }
        let mut deletes: Vec<EntityKey> = self
            .baseline
            .keys()
            .filter(|key| !seen.contains_key(*key))
            .cloned()
            .collect();
        deletes.sort();
        changes.deletes = deletes;
        Ok(changes)
    }

    /// Commit the operation's changes to `store` and make them the new baseline,
    /// pruning stored run history by the workspace's [`RunRetention`]. Returns
    /// what was written.
    pub fn commit(&mut self, store: &mut impl Store) -> Result<ChangeSet, StoreError> {
        let changes = self.changes()?;
        if changes.is_empty() {
            return Ok(changes);
        }
        store.apply(&changes)?;
        // Count limits can only be newly exceeded in workflows whose runs were just
        // archived; age limits are checked on every commit (an indexed lookup).
        let mut workflows: Vec<String> = changes
            .upserts
            .iter()
            .filter(|r| r.archived)
            .filter_map(|r| r.columns.as_ref().map(|c| c.workflow_id.clone()))
            .collect();
        workflows.sort();
        workflows.dedup();
        let retention = self.workspace.retention;
        if !workflows.is_empty() || retention.created_before.is_some() {
            store.prune_runs(&retention, &workflows)?;
        }
        if let Some(meta) = &changes.meta {
            self.meta = Some(meta.clone());
        }
        for key in &changes.deletes {
            self.baseline.remove(key);
        }
        for record in &changes.upserts {
            if record.archived {
                self.baseline.remove(&record.key);
            } else {
                self.baseline
                    .insert(record.key.clone(), (record.order, digest(&record.body)));
            }
            if record.key.collection == Collection::Run {
                self.run_floor = Some(self.run_floor.map_or(record.order, |f| f.min(record.order)));
            }
        }
        Ok(changes)
    }
}

/// How long finished runs are kept as history, as configured for a host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetentionPolicy {
    /// Finished runs kept per workflow, newest first. `None` keeps any number.
    pub per_workflow: Option<usize>,
    /// Finished runs older than this (from when they were created) are deleted.
    /// `None` keeps them however old.
    pub max_age_ms: Option<u64>,
    /// The same for failed runs, so they can be kept longer for investigation.
    pub failed_max_age_ms: Option<u64>,
}
impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            per_workflow: Some(DEFAULT_RUN_HISTORY),
            max_age_ms: None,
            failed_max_age_ms: None,
        }
    }
}
/// Finished runs kept per workflow unless the host configures otherwise.
pub const DEFAULT_RUN_HISTORY: usize = 100;
const DAY_MS: u64 = 24 * 60 * 60 * 1000;
impl RetentionPolicy {
    /// Parse the host's settings: `RUN_HISTORY` (runs per workflow; empty for the
    /// default, `0` or `unlimited` for no count limit), `RUN_HISTORY_DAYS` (empty,
    /// `0` or `unlimited` for no age limit) and `FAILED_RUN_HISTORY_DAYS` (empty to
    /// follow `RUN_HISTORY_DAYS`). Anything else is a configuration error.
    pub fn parse(history: &str, days: &str, failed_days: &str) -> Result<Self, String> {
        let number = |name: &str, value: &str| -> Result<Option<u64>, String> {
            match value.trim() {
                "0" | "unlimited" => Ok(None),
                text => text
                    .parse::<u64>()
                    .map(Some)
                    .map_err(|_| format!("{name} must be a positive integer, 0 or unlimited")),
            }
        };
        let per_workflow = match history.trim() {
            "" => Some(DEFAULT_RUN_HISTORY),
            text => number("RUN_HISTORY", text)?.map(|n| n as usize),
        };
        let days_ms = |name: &str, value: &str| -> Result<Option<u64>, String> {
            Ok(number(name, value)?.map(|d| d.saturating_mul(DAY_MS)))
        };
        let max_age_ms = match days.trim() {
            "" => None,
            text => days_ms("RUN_HISTORY_DAYS", text)?,
        };
        let failed_max_age_ms = match failed_days.trim() {
            "" => max_age_ms,
            text => days_ms("FAILED_RUN_HISTORY_DAYS", text)?,
        };
        Ok(Self {
            per_workflow,
            max_age_ms,
            failed_max_age_ms,
        })
    }
    /// The policy's cutoffs at `now`.
    pub fn at(&self, now: u64) -> RunRetention {
        RunRetention {
            per_workflow: self.per_workflow,
            created_before: self.max_age_ms.map(|age| now.saturating_sub(age)),
            failed_created_before: self.failed_max_age_ms.map(|age| now.saturating_sub(age)),
        }
    }
}
/// A [`RetentionPolicy`] resolved to absolute cutoffs, carried by a loaded
/// workspace and applied when it commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunRetention {
    pub per_workflow: Option<usize>,
    /// Finished runs created before this time are deleted.
    pub created_before: Option<u64>,
    /// Failed runs created before this time are deleted.
    pub failed_created_before: Option<u64>,
}
impl Default for RunRetention {
    fn default() -> Self {
        RetentionPolicy::default().at(0)
    }
}
impl RunRetention {
    /// Whether a finished run with this status and creation time has expired.
    pub fn expired(&self, status: &str, created_at: u64) -> bool {
        let cutoff = if status == "failed" {
            self.failed_created_before
        } else {
            self.created_before
        };
        cutoff.is_some_and(|before| created_at < before)
    }
}

type Entities = Vec<(EntityKey, String)>;
/// SHA-256 of an entity's encoded body.
type Digest = [u8; 32];
fn digest(body: &str) -> Digest {
    use sha2::{Digest as _, Sha256};
    Sha256::digest(body.as_bytes()).into()
}

/// Split a workspace into its metadata entity and keyed collection entities, each
/// in document order.
fn split(workspace: &Workspace) -> Result<(String, Vec<(Collection, Entities)>), StoreError> {
    let mut collections = Vec::new();
    let mut keyed = |collection: Collection, items: Vec<(String, String)>| {
        let mut unique = HashMap::new();
        let mut entities = Vec::with_capacity(items.len());
        for (id, body) in items {
            let key = EntityKey { collection, id };
            if unique.insert(key.clone(), ()).is_some() {
                return Err(StoreError::DuplicateKey(key));
            }
            entities.push((key, body));
        }
        collections.push((collection, entities));
        Ok(())
    };
    keyed(
        Collection::Workflow,
        workspace
            .workflows
            .iter()
            .map(|w| Ok((w.graph.id.clone(), encode(w)?)))
            .collect::<Result<_, StoreError>>()?,
    )?;
    keyed(
        Collection::Revision,
        workspace
            .revisions
            .iter()
            .map(|r| Ok((format!("{}/{}", r.workflow_id, r.revision), encode(r)?)))
            .collect::<Result<_, StoreError>>()?,
    )?;
    keyed(
        Collection::Run,
        workspace
            .runs
            .iter()
            .map(|r| {
                let id = r
                    .id_string()
                    .map_err(|_| StoreError::MissingKey(Collection::Run))?;
                Ok((id, encode(r)?))
            })
            .collect::<Result<_, StoreError>>()?,
    )?;
    keyed(
        Collection::Audit,
        workspace
            .audit
            .iter()
            .map(|entry| {
                let id = entry
                    .get("id")
                    .and_then(Value::as_u64)
                    .ok_or(StoreError::MissingKey(Collection::Audit))?;
                Ok((id.to_string(), encode(entry)?))
            })
            .collect::<Result<_, StoreError>>()?,
    )?;
    keyed(
        Collection::Webhook,
        workspace
            .webhooks
            .iter()
            .map(|w| Ok((w.endpoint_id.clone(), encode(w)?)))
            .collect::<Result<_, StoreError>>()?,
    )?;
    Ok((split_meta(workspace)?, collections))
}
/// The metadata entity: the workspace without its keyed collections.
fn split_meta(workspace: &Workspace) -> Result<String, StoreError> {
    let mut meta =
        serde_json::to_value(workspace).map_err(|e| StoreError::Invalid(e.to_string()))?;
    let object = meta
        .as_object_mut()
        .ok_or_else(|| StoreError::Invalid("workspace is not an object".into()))?;
    for collection in Collection::ALL {
        object.remove(collection.field());
    }
    Ok(meta.to_string())
}
/// The size of the workspace encoded as one document, computed from its parts:
/// the metadata object plus `,"field":[...]` for each collection.
fn encoded_total(meta: &str, collections: &[(Collection, Entities)]) -> usize {
    meta.len()
        + collections
            .iter()
            .map(|(collection, entities)| {
                let items: usize = entities.iter().map(|(_, body)| body.len()).sum();
                // `,"field":[` + items + commas between them + `]`
                1 + collection.field().len() + 2 + 2 + items + entities.len().saturating_sub(1) + 1
            })
            .sum::<usize>()
}

/// Order keys for `entities` (in document order) that keep every stored entity's
/// existing key where possible. New entities take keys between their neighbours.
/// If stored entities were reordered, or there is no room between neighbours,
/// the whole collection is renumbered.
fn assign_orders(
    entities: &Entities,
    baseline: &HashMap<EntityKey, (i64, Digest)>,
    floor: Option<i64>,
) -> Vec<i64> {
    let existing: Vec<Option<i64>> = entities
        .iter()
        .map(|(key, _)| baseline.get(key).map(|(order, _)| *order))
        .collect();
    let mut orders = Vec::with_capacity(entities.len());
    let mut index = 0;
    let mut previous: Option<i64> = None;
    while index < existing.len() {
        if let Some(order) = existing[index] {
            if previous.is_some_and(|p| p >= order) {
                return renumber(entities.len(), floor);
            }
            orders.push(order);
            previous = Some(order);
            index += 1;
            continue;
        }
        // A run of new entities between `previous` and the next stored entity.
        let start = index;
        while index < existing.len() && existing[index].is_none() {
            index += 1;
        }
        let count = (index - start) as i64;
        let next = existing.get(index).copied().flatten();
        // Leading new entities also sort before every stored entity, including
        // ones not loaded (archived runs).
        let next = match (previous, next, floor) {
            (None, Some(next), Some(floor)) => Some(next.min(floor)),
            (None, None, Some(floor)) => Some(floor),
            _ => next,
        };
        let keys: Option<Vec<i64>> = match (previous, next) {
            (None, None) => Some((1..=count).map(|i| i * ORDER_SPACING).collect()),
            (Some(low), None) => (1..=count)
                .map(|i| low.checked_add(i.checked_mul(ORDER_SPACING)?))
                .collect(),
            (None, Some(high)) => (0..count)
                .map(|i| high.checked_sub((count - i).checked_mul(ORDER_SPACING)?))
                .collect(),
            (Some(low), Some(high)) => {
                let step = (high - low) / (count + 1);
                (step > 0).then(|| (1..=count).map(|i| low + i * step).collect())
            }
        };
        match keys {
            Some(keys) => {
                previous = keys.last().copied().or(previous);
                orders.extend(keys);
            }
            None => return renumber(entities.len(), floor),
        }
    }
    orders
}
fn encode<T: serde::Serialize>(value: &T) -> Result<String, StoreError> {
    serde_json::to_string(value).map_err(|e| StoreError::Invalid(e.to_string()))
}
/// Fresh keys for a whole collection, below `floor` when there is one.
fn renumber(len: usize, floor: Option<i64>) -> Vec<i64> {
    let len = len as i64;
    match floor {
        Some(floor) => (0..len)
            .map(|i| floor.saturating_sub((len - i).saturating_mul(ORDER_SPACING)))
            .collect(),
        None => (1..=len).map(|i| i * ORDER_SPACING).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entities(ids: &[&str]) -> Entities {
        ids.iter()
            .map(|id| {
                (
                    EntityKey {
                        collection: Collection::Run,
                        id: (*id).into(),
                    },
                    String::new(),
                )
            })
            .collect()
    }
    fn baseline(pairs: &[(&str, i64)]) -> HashMap<EntityKey, (i64, Digest)> {
        pairs
            .iter()
            .map(|(id, order)| {
                (
                    EntityKey {
                        collection: Collection::Run,
                        id: (*id).into(),
                    },
                    (*order, [0; 32]),
                )
            })
            .collect()
    }
    fn increasing(orders: &[i64]) -> bool {
        orders.windows(2).all(|w| w[0] < w[1])
    }

    #[test]
    fn new_entities_take_keys_without_moving_stored_ones() {
        let stored = baseline(&[("b", 100), ("c", 200)]);
        // Prepended, appended and inserted between stored entities.
        let orders = assign_orders(&entities(&["a", "b", "x", "c", "d"]), &stored, None);
        assert!(increasing(&orders), "{orders:?}");
        assert_eq!((orders[1], orders[3]), (100, 200), "stored keys unchanged");
    }

    #[test]
    fn reordered_or_crowded_collections_are_renumbered() {
        let stored = baseline(&[("a", 1), ("b", 2)]);
        // Reordered stored entities.
        assert_eq!(
            assign_orders(&entities(&["b", "a"]), &stored, None),
            renumber(2, None)
        );
        // No integer room between 1 and 2 for a new entity.
        assert_eq!(
            assign_orders(&entities(&["a", "x", "b"]), &stored, None),
            renumber(3, None)
        );
    }

    #[test]
    fn encoded_total_is_the_whole_document_length() {
        let mut ws = Workspace::default();
        assert_eq!(
            encoded_total(&split(&ws).unwrap().0, &split(&ws).unwrap().1),
            serde_json::to_string(&ws).unwrap().len(),
            "empty"
        );
        let doc = serde_json::json!({"schemaVersion":1,"id":"flow","name":"Flow","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Manual Trigger","label":"Start","config":{}}],"edges":[]});
        ws.handle(
            "PUT",
            "/api/workflows/flow",
            doc.to_string().as_bytes(),
            0,
            "",
        )
        .unwrap();
        ws.handle("POST", "/api/workflows/flow/publish", b"", 0, "")
            .unwrap();
        for i in 0..3 {
            let body = serde_json::json!({"workflowId":"flow","mode":"run","input":{"i":i}});
            ws.handle(
                "POST",
                "/api/runs",
                body.to_string().as_bytes(),
                i,
                &format!("r{i}"),
            )
            .unwrap();
        }
        let hook = serde_json::json!({"schemaVersion":1,"id":"hook","name":"Hook","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Webhook","label":"Start","config":{"authentication":"bearer"}}],"edges":[]});
        ws.handle(
            "PUT",
            "/api/workflows/hook",
            hook.to_string().as_bytes(),
            0,
            "",
        )
        .unwrap();
        ws.configure_webhook("hook", "endpoint".into(), "hash".into())
            .unwrap();
        let (meta, collections) = split(&ws).unwrap();
        assert!(
            collections.iter().all(|(_, e)| !e.is_empty()),
            "every collection populated"
        );
        assert_eq!(
            encoded_total(&meta, &collections),
            serde_json::to_string(&ws).unwrap().len()
        );
    }

    #[test]
    fn new_runs_sort_before_runs_that_are_not_loaded() {
        // "b" is loaded at 100; an archived run (not loaded) sits at 40.
        let stored = baseline(&[("b", 100)]);
        let orders = assign_orders(&entities(&["new", "b"]), &stored, Some(40));
        assert!(orders[0] < 40 && orders[1] == 100, "{orders:?}");
        // With nothing loaded, new runs still sort first.
        let orders = assign_orders(&entities(&["x", "y"]), &HashMap::new(), Some(40));
        assert!(orders[0] < orders[1] && orders[1] < 40, "{orders:?}");
        // Renumbering keeps the whole collection below the floor.
        let orders = assign_orders(
            &entities(&["b", "a"]),
            &baseline(&[("a", 1), ("b", 2)]),
            Some(-5),
        );
        assert!(
            orders.iter().all(|o| *o < -5) && orders[0] < orders[1],
            "{orders:?}"
        );
    }

    #[test]
    fn empty_and_all_new_collections_get_spaced_keys() {
        assert!(assign_orders(&entities(&[]), &HashMap::new(), None).is_empty());
        assert_eq!(
            assign_orders(&entities(&["a", "b"]), &HashMap::new(), None),
            vec![ORDER_SPACING, 2 * ORDER_SPACING]
        );
    }
}
