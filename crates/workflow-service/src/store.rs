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
}

/// Indexed facts about a run, derived from its [`RunRecord`] whenever the run is
/// written. They answer every [`RunQueries`] question without decoding the run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunColumns {
    /// Engine status (`running`, `paused`, `succeeded`, `failed`, `cancelled`).
    pub status: String,
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
        Ok(Self {
            status: status_text(record.status()?),
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
            summary: record.view(true)?.to_string(),
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
    /// Every record of a collection, sorted by ascending `order`.
    fn records(&self, collection: Collection) -> Result<Vec<Record>, StoreError>;
    /// One record by key.
    fn record(&self, key: &EntityKey) -> Result<Option<Record>, StoreError>;
    /// Apply a change set atomically: all of it or none of it.
    fn apply(&mut self, changes: &ChangeSet) -> Result<(), StoreError>;
}

/// An in-memory [`Store`] for tests and hosts without durable storage.
#[derive(Clone, Debug, Default)]
pub struct MemoryStore {
    meta: Option<String>,
    records: BTreeMap<EntityKey, (i64, String, Option<RunColumns>)>,
}
impl MemoryStore {
    fn run_columns(&self) -> impl Iterator<Item = (&EntityKey, i64, &RunColumns)> {
        let mut runs: Vec<_> = self
            .records
            .iter()
            .filter(|(key, _)| key.collection == Collection::Run)
            .filter_map(|(key, (order, _, columns))| columns.as_ref().map(|c| (key, *order, c)))
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
            .filter(|(key, _)| key.collection == collection)
            .map(|(key, (order, body, columns))| Record {
                key: key.clone(),
                order: *order,
                body: body.clone(),
                columns: columns.clone(),
            })
            .collect();
        records.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.key.cmp(&b.key)));
        Ok(records)
    }
    fn record(&self, key: &EntityKey) -> Result<Option<Record>, StoreError> {
        Ok(self.records.get(key).map(|(order, body, columns)| Record {
            key: key.clone(),
            order: *order,
            body: body.clone(),
            columns: columns.clone(),
        }))
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
                (record.order, record.body.clone(), record.columns.clone()),
            );
        }
        Ok(())
    }
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
    baseline: HashMap<EntityKey, (i64, String)>,
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
                baseline.insert(record.key, (record.order, record.body));
            }
            object.insert(collection.field().into(), Value::Array(items));
        }
        let workspace =
            Workspace::from_value(root, encoded).map_err(|e| StoreError::Invalid(e.message))?;
        Ok(Self {
            workspace,
            meta,
            baseline,
        })
    }

    /// Apply retention, then return the rows that differ from what was loaded.
    /// The tracked baseline is not changed.
    ///
    /// Each entity is encoded once. Retention works from those encoded sizes, so
    /// the whole workspace is never encoded as one document.
    pub fn changes(&mut self) -> Result<ChangeSet, StoreError> {
        let (mut meta, mut collections) = split(&self.workspace)?;
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
            .map_err(|e| StoreError::Invalid(e.message))?;
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
        let mut changes = ChangeSet {
            meta: (self.meta.as_deref() != Some(meta.as_str())).then_some(meta),
            ..ChangeSet::default()
        };
        let mut seen = HashMap::new();
        for (collection, entities) in collections {
            let orders = assign_orders(&entities, &self.baseline);
            for (index, ((key, body), order)) in entities.into_iter().zip(orders).enumerate() {
                let unchanged = self
                    .baseline
                    .get(&key)
                    .is_some_and(|(old_order, old_body)| *old_order == order && *old_body == body);
                seen.insert(key.clone(), ());
                if !unchanged {
                    // Run columns are derived only for runs being written.
                    let columns = match collection {
                        Collection::Run => Some(
                            RunColumns::of(&self.workspace.runs[index])
                                .map_err(|e| StoreError::Invalid(e.message))?,
                        ),
                        _ => None,
                    };
                    changes.upserts.push(Record {
                        key,
                        order,
                        body,
                        columns,
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

    /// Commit the operation's changes to `store` and make them the new baseline.
    /// Returns what was written.
    pub fn commit(&mut self, store: &mut impl Store) -> Result<ChangeSet, StoreError> {
        let changes = self.changes()?;
        if changes.is_empty() {
            return Ok(changes);
        }
        store.apply(&changes)?;
        if let Some(meta) = &changes.meta {
            self.meta = Some(meta.clone());
        }
        for key in &changes.deletes {
            self.baseline.remove(key);
        }
        for record in &changes.upserts {
            self.baseline
                .insert(record.key.clone(), (record.order, record.body.clone()));
        }
        Ok(changes)
    }
}

type Entities = Vec<(EntityKey, String)>;

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
fn assign_orders(entities: &Entities, baseline: &HashMap<EntityKey, (i64, String)>) -> Vec<i64> {
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
                return renumber(entities.len());
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
            None => return renumber(entities.len()),
        }
    }
    orders
}
fn encode<T: serde::Serialize>(value: &T) -> Result<String, StoreError> {
    serde_json::to_string(value).map_err(|e| StoreError::Invalid(e.to_string()))
}
fn renumber(len: usize) -> Vec<i64> {
    (1..=len as i64).map(|i| i * ORDER_SPACING).collect()
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
    fn baseline(pairs: &[(&str, i64)]) -> HashMap<EntityKey, (i64, String)> {
        pairs
            .iter()
            .map(|(id, order)| {
                (
                    EntityKey {
                        collection: Collection::Run,
                        id: (*id).into(),
                    },
                    (*order, String::new()),
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
        let orders = assign_orders(&entities(&["a", "b", "x", "c", "d"]), &stored);
        assert!(increasing(&orders), "{orders:?}");
        assert_eq!((orders[1], orders[3]), (100, 200), "stored keys unchanged");
    }

    #[test]
    fn reordered_or_crowded_collections_are_renumbered() {
        let stored = baseline(&[("a", 1), ("b", 2)]);
        // Reordered stored entities.
        assert_eq!(assign_orders(&entities(&["b", "a"]), &stored), renumber(2));
        // No integer room between 1 and 2 for a new entity.
        assert_eq!(
            assign_orders(&entities(&["a", "x", "b"]), &stored),
            renumber(3)
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
    fn empty_and_all_new_collections_get_spaced_keys() {
        assert!(assign_orders(&entities(&[]), &HashMap::new()).is_empty());
        assert_eq!(
            assign_orders(&entities(&["a", "b"]), &HashMap::new()),
            vec![ORDER_SPACING, 2 * ORDER_SPACING]
        );
    }
}
