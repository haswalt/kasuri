//! Per-entity workspace storage boundary (stage 1, phase 1 of
//! `docs/workspace-storage.md`).
//!
//! A [`Workspace`] is split into keyed entities: workflows, revisions, runs, audit
//! entries and webhook registrations, plus one metadata entity holding everything
//! else. A [`Store`] holds those records. [`Tracked`] loads a workspace from a
//! store and, after an operation, commits only the entities that changed.
//!
//! This phase changes no storage format and no service behaviour: the service
//! still operates on a whole `Workspace`, and hosts still persist it as they do
//! today. Later phases load entities on demand and back the trait with SQLite.
use crate::Workspace;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub key: EntityKey,
    pub order: i64,
    pub body: String,
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
    records: BTreeMap<EntityKey, (i64, String)>,
}
impl MemoryStore {
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
            .map(|(key, (order, body))| Record {
                key: key.clone(),
                order: *order,
                body: body.clone(),
            })
            .collect();
        records.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.key.cmp(&b.key)));
        Ok(records)
    }
    fn record(&self, key: &EntityKey) -> Result<Option<Record>, StoreError> {
        Ok(self.records.get(key).map(|(order, body)| Record {
            key: key.clone(),
            order: *order,
            body: body.clone(),
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
            self.records
                .insert(record.key.clone(), (record.order, record.body.clone()));
        }
        Ok(())
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

    /// Apply today's retention (`Workspace::checkpoint`), then return the rows that
    /// differ from what was loaded. The tracked baseline is not changed.
    pub fn changes(&mut self) -> Result<ChangeSet, StoreError> {
        self.workspace
            .checkpoint()
            .map_err(|e| StoreError::Invalid(e.message))?;
        let (meta, collections) = split(&self.workspace)?;
        let mut changes = ChangeSet {
            meta: (self.meta.as_deref() != Some(meta.as_str())).then_some(meta),
            ..ChangeSet::default()
        };
        let mut seen = HashMap::new();
        for (_, entities) in collections {
            let orders = assign_orders(&entities, &self.baseline);
            for ((key, body), order) in entities.into_iter().zip(orders) {
                let unchanged = self
                    .baseline
                    .get(&key)
                    .is_some_and(|(old_order, old_body)| *old_order == order && *old_body == body);
                seen.insert(key.clone(), ());
                if !unchanged {
                    changes.upserts.push(Record { key, order, body });
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
    let mut meta =
        serde_json::to_value(workspace).map_err(|e| StoreError::Invalid(e.to_string()))?;
    let object = meta
        .as_object_mut()
        .ok_or_else(|| StoreError::Invalid("workspace is not an object".into()))?;
    for collection in Collection::ALL {
        object.remove(collection.field());
    }
    Ok((meta.to_string(), collections))
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
    fn empty_and_all_new_collections_get_spaced_keys() {
        assert!(assign_orders(&entities(&[]), &HashMap::new()).is_empty());
        assert_eq!(
            assign_orders(&entities(&["a", "b"]), &HashMap::new()),
            vec![ORDER_SPACING, 2 * ORDER_SPACING]
        );
    }
}
