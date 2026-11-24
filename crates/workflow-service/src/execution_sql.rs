//! A [`RunStore`] and [`WorkBus`] on plain SQL, for hosts outside Cloudflare
//! (stage 2, item 9 of `docs/workspace-storage.md`).
//!
//! Both are written against [`SqlExec`], so they run on any SQL database the host
//! connects ([`SqlExec`] adapters translate the positional `?` parameters, for
//! example to Postgres's `$1`). The statements use `INSERT ... ON CONFLICT`,
//! `UPDATE ... RETURNING` and `DELETE`, which SQLite (3.35 and later) and Postgres
//! share.
//!
//! - [`SqlRunStore`] keeps each run tree as one row with a version column; a save
//!   is a compare-and-swap on it.
//! - [`SqlWorkBus`] is an outbox table. A consumer [`receive`](SqlWorkBus::receive)s
//!   due messages, which leases them, and [`ack`](SqlWorkBus::ack)s each once
//!   handled. A consumer that fails without acknowledging loses its lease, and the
//!   message is delivered again: at-least-once delivery, as the execution core
//!   expects. Several consumers can share the table; on Postgres, the host can add
//!   `FOR UPDATE SKIP LOCKED` to the leasing subquery to avoid contention.
//!
//! The bus can equally be a real broker (SQS, NATS, RabbitMQ): only `send`'s
//! delay and redelivery of unacknowledged messages are needed.
use super::{RunStore, RunTree, SaveError, WorkBus, WorkMessage};
use crate::store::StoreError;
use crate::store::sql::{SqlExec, SqlValue};
use crate::{ApiError, error};
use std::cell::Cell;

const TREES: &str = "CREATE TABLE IF NOT EXISTS exec_trees (root TEXT PRIMARY KEY, version INTEGER NOT NULL, body TEXT NOT NULL)";
const MESSAGES: &str = "CREATE TABLE IF NOT EXISTS exec_messages (id INTEGER PRIMARY KEY, root TEXT NOT NULL, version INTEGER NOT NULL, due_at INTEGER NOT NULL, leased_until INTEGER NOT NULL DEFAULT 0)";
const DUE: &str =
    "CREATE INDEX IF NOT EXISTS exec_messages_due ON exec_messages(due_at, leased_until)";

fn failed(e: StoreError) -> ApiError {
    error(500, e.to_string())
}
fn integer(value: u64) -> SqlValue {
    SqlValue::Integer(value.min(i64::MAX as u64) as i64)
}
fn text(value: &str) -> SqlValue {
    SqlValue::Text(value.to_owned())
}

/// Run trees in a SQL table, one row each, with compare-and-swap on a version.
pub struct SqlRunStore<E: SqlExec> {
    sql: E,
}
impl<E: SqlExec> SqlRunStore<E> {
    /// Use `sql`, creating the table if needed.
    pub fn new(sql: E) -> Result<Self, ApiError> {
        sql.query(TREES, Vec::new()).map_err(failed)?;
        Ok(Self { sql })
    }
    /// Remove a finished tree (the host does this once the workspace's index has
    /// its final state).
    pub fn delete(&self, root: &str) -> Result<(), ApiError> {
        self.sql
            .query("DELETE FROM exec_trees WHERE root=?", vec![text(root)])
            .map_err(failed)?;
        Ok(())
    }
}
impl<E: SqlExec> RunStore for SqlRunStore<E> {
    fn load(&self, root: &str) -> Result<Option<(RunTree, u64)>, ApiError> {
        let rows = self
            .sql
            .query(
                "SELECT version, body FROM exec_trees WHERE root=?",
                vec![text(root)],
            )
            .map_err(failed)?;
        let Some(mut row) = rows.into_iter().next() else {
            return Ok(None);
        };
        let body = row.pop().ok_or_else(|| error(500, "empty row"))?;
        let version = row
            .pop()
            .ok_or_else(|| error(500, "empty row"))?
            .integer()
            .map_err(failed)?
            .unwrap_or(0) as u64;
        let tree = serde_json::from_str(&body.text().map_err(failed)?)
            .map_err(|e| error(500, e.to_string()))?;
        Ok(Some((tree, version)))
    }
    fn save(&self, root: &str, tree: &RunTree, expected: Option<u64>) -> Result<u64, SaveError> {
        let body = serde_json::to_string(tree).map_err(|e| SaveError::Failed(e.to_string()))?;
        let version = expected.unwrap_or(0) + 1;
        let rows = match expected {
            None => self.sql.query(
                "INSERT INTO exec_trees(root, version, body) VALUES(?, ?, ?) ON CONFLICT(root) DO NOTHING RETURNING root",
                vec![text(root), integer(version), SqlValue::Text(body)],
            ),
            Some(expected) => self.sql.query(
                "UPDATE exec_trees SET version=?, body=? WHERE root=? AND version=? RETURNING root",
                vec![integer(version), SqlValue::Text(body), text(root), integer(expected)],
            ),
        }
        .map_err(|e| SaveError::Failed(e.to_string()))?;
        if rows.is_empty() {
            return Err(SaveError::Conflict);
        }
        Ok(version)
    }
}

/// A received message: handle it, then [`ack`](SqlWorkBus::ack) its `id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivery {
    pub id: i64,
    pub message: WorkMessage,
}

/// An outbox table as a [`WorkBus`]. `send` stores the message with the time it
/// becomes due, relative to the bus's clock ([`SqlWorkBus::set_now`]).
pub struct SqlWorkBus<E: SqlExec> {
    sql: E,
    now: Cell<u64>,
}
impl<E: SqlExec> SqlWorkBus<E> {
    /// Use `sql`, creating the table if needed.
    pub fn new(sql: E, now: u64) -> Result<Self, ApiError> {
        sql.query(MESSAGES, Vec::new()).map_err(failed)?;
        sql.query(DUE, Vec::new()).map_err(failed)?;
        Ok(Self {
            sql,
            now: Cell::new(now),
        })
    }
    /// The time `send` measures delays from: the host's clock when it handles a
    /// message.
    pub fn set_now(&self, now: u64) {
        self.now.set(now);
    }
    /// Lease up to `limit` messages due at `now` (and not leased) until
    /// `now + lease_ms`, oldest due first. A message not acknowledged by then is
    /// delivered again.
    pub fn receive(
        &self,
        now: u64,
        lease_ms: u64,
        limit: usize,
    ) -> Result<Vec<Delivery>, ApiError> {
        let rows = self
            .sql
            .query(
                "UPDATE exec_messages SET leased_until=? WHERE id IN (SELECT id FROM exec_messages WHERE due_at<=? AND leased_until<=? ORDER BY due_at, id LIMIT ?) RETURNING id, root, version, due_at",
                vec![
                    integer(now.saturating_add(lease_ms)),
                    integer(now),
                    integer(now),
                    integer(limit as u64),
                ],
            )
            .map_err(failed)?;
        let mut deliveries = rows
            .into_iter()
            .map(|row| {
                let mut row = row.into_iter();
                let mut next = || row.next().ok_or_else(|| error(500, "short row"));
                let id = next()?.integer().map_err(failed)?.unwrap_or(0);
                let run = next()?.text().map_err(failed)?;
                let version = next()?.integer().map_err(failed)?.unwrap_or(0) as u64;
                let due = next()?.integer().map_err(failed)?.unwrap_or(0);
                Ok((
                    due,
                    Delivery {
                        id,
                        message: WorkMessage { run, version },
                    },
                ))
            })
            .collect::<Result<Vec<_>, ApiError>>()?;
        // RETURNING rows come in no particular order.
        deliveries.sort_by_key(|(due, d)| (*due, d.id));
        Ok(deliveries.into_iter().map(|(_, d)| d).collect())
    }
    /// The message was handled: remove it.
    pub fn ack(&self, id: i64) -> Result<(), ApiError> {
        self.sql
            .query(
                "DELETE FROM exec_messages WHERE id=?",
                vec![SqlValue::Integer(id)],
            )
            .map_err(failed)?;
        Ok(())
    }
    /// When the next message becomes due, if any (for a host's timer).
    pub fn next_due(&self) -> Result<Option<u64>, ApiError> {
        let rows = self
            .sql
            .query(
                "SELECT min(max(due_at, leased_until)) FROM exec_messages",
                Vec::new(),
            )
            .map_err(failed)?;
        Ok(rows
            .into_iter()
            .next()
            .and_then(|row| row.into_iter().next())
            .map(|v| v.integer())
            .transpose()
            .map_err(failed)?
            .flatten()
            .map(|at| at as u64))
    }
}
impl<E: SqlExec> WorkBus for SqlWorkBus<E> {
    fn send(&self, message: WorkMessage, delay_ms: Option<u64>) -> Result<(), ApiError> {
        self.sql
            .query(
                "INSERT INTO exec_messages(root, version, due_at) VALUES(?, ?, ?)",
                vec![
                    text(&message.run),
                    integer(message.version),
                    integer(self.now.get().saturating_add(delay_ms.unwrap_or(0))),
                ],
            )
            .map_err(failed)?;
        Ok(())
    }
}
