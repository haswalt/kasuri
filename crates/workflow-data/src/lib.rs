//! Runtime-independent Data Tables and Context Store semantics.
//! Hosts must execute apply() inside one storage transaction, including its receipt.
mod schema;
pub use schema::{command_schema, mutation_schema, query_schema};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const MAX_VALUE_BYTES: usize = 32_768;
pub const MAX_PAGE_SIZE: usize = 100;
pub const MAX_SCAN_SIZE: usize = 1000;
pub const MAX_SCAN_BYTES: usize = 1_048_576;
const MAX_QUERY_BYTES: usize = 60_000;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableKind {
    Table,
    Context,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Number,
    Boolean,
    Object,
    Array,
    Any,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Field {
    pub name: String,
    pub kind: FieldType,
    #[serde(default)]
    pub required: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Definition {
    pub id: String,
    pub name: String,
    pub kind: TableKind,
    #[serde(default)]
    pub fields: Vec<Field>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub definition: Definition,
    pub version: u64,
    pub data_revision: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub key: String,
    pub value: Value,
    pub version: u64,
    pub expires_at: Option<u64>,
}
impl Row {
    pub fn live(&self, now: u64) -> bool {
        self.expires_at.is_none_or(|at| at > now)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Eq,
    Ne,
    Gt,
    Lt,
    Exists,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Filter {
    pub pointer: String,
    pub operator: Operator,
    #[serde(default)]
    pub value: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Query {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "page_size")]
    pub limit: usize,
    #[serde(default)]
    pub filters: Vec<Filter>,
}
fn page_size() -> usize {
    25
}
impl Default for Query {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: 25,
            filters: vec![],
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Command {
    CreateTable {
        definition: Definition,
    },
    RenameTable {
        table_id: String,
        name: String,
        expected_version: u64,
    },
    GetTable {
        table_id: String,
    },
    Get {
        table_id: String,
        key: String,
    },
    InsertIfAbsent {
        table_id: String,
        key: String,
        value: Value,
        #[serde(default)]
        ttl_ms: Option<u64>,
    },
    Put {
        table_id: String,
        key: String,
        value: Value,
        #[serde(default)]
        expected_version: Option<u64>,
        #[serde(default)]
        ttl_ms: Option<u64>,
    },
    Patch {
        table_id: String,
        key: String,
        patch: serde_json::Map<String, Value>,
        #[serde(default)]
        remove_fields: Vec<String>,
        #[serde(default)]
        expected_version: Option<u64>,
    },
    Delete {
        table_id: String,
        key: String,
        #[serde(default)]
        expected_version: Option<u64>,
    },
    Query {
        table_id: String,
        #[serde(default)]
        query: Query,
    },
}
impl Command {
    pub fn table_id(&self) -> &str {
        match self {
            Self::CreateTable { definition } => &definition.id,
            Self::RenameTable { table_id, .. }
            | Self::GetTable { table_id }
            | Self::Get { table_id, .. }
            | Self::InsertIfAbsent { table_id, .. }
            | Self::Put { table_id, .. }
            | Self::Patch { table_id, .. }
            | Self::Delete { table_id, .. }
            | Self::Query { table_id, .. } => table_id,
        }
    }
    pub fn mutates(&self) -> bool {
        !matches!(
            self,
            Self::Get { .. } | Self::GetTable { .. } | Self::Query { .. }
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    NotFound,
    Conflict,
    Storage,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataError {
    pub code: ErrorCode,
    pub message: String,
}
pub type Result<T> = std::result::Result<T, DataError>;
pub fn error(code: ErrorCode, message: &str) -> DataError {
    DataError {
        code,
        message: message.into(),
    }
}
impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for DataError {}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub key: String,
    pub fingerprint: String,
    pub result: Value,
    /// Original host clock when this effect committed, never the replay time.
    /// Legacy receipts have no trustworthy completion time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
}
/// All methods are synchronous. A host transaction must roll back every write
/// if any method fails; receipts must not commit separately from row mutations.
#[derive(Debug, Clone)]
pub struct ScanPage {
    pub rows: Vec<Row>,
    pub more: bool,
}
pub trait Store {
    fn table(&mut self, id: &str) -> Result<Option<Table>>;
    fn save_table(&mut self, table: Table) -> Result<()>;
    fn row(&mut self, table: &str, key: &str) -> Result<Option<Row>>;
    fn save_row(&mut self, table: &str, row: Row) -> Result<()>;
    fn remove_row(&mut self, table: &str, key: &str) -> Result<()>;
    /// Key-ordered, exclusive cursor, including physically retained expired rows.
    /// Return at most limit rows and MAX_SCAN_BYTES, with explicit continuation.
    fn scan(&mut self, table: &str, after: Option<&str>, limit: usize) -> Result<ScanPage>;
    fn receipt(&mut self, key: &str) -> Result<Option<Receipt>>;
    fn save_receipt(&mut self, receipt: Receipt) -> Result<()>;
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 512 && !key.chars().any(char::is_control)
}
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 160 && !name.chars().any(char::is_control)
}
fn validate_definition(definition: &Definition) -> Result<()> {
    if !valid_id(&definition.id) || !valid_name(&definition.name) || definition.fields.len() > 100 {
        return Err(error(ErrorCode::InvalidRequest, "Invalid table definition"));
    }
    let mut fields = std::collections::BTreeSet::new();
    for field in &definition.fields {
        if !valid_id(&field.name) || !fields.insert(&field.name) {
            return Err(error(
                ErrorCode::InvalidRequest,
                "Field names must be unique valid identifiers",
            ));
        }
    }
    if definition.kind == TableKind::Context && !definition.fields.is_empty() {
        return Err(error(
            ErrorCode::InvalidRequest,
            "Context namespaces hold arbitrary JSON values without table fields",
        ));
    }
    Ok(())
}
fn validate_value(table: &Table, value: &Value) -> Result<()> {
    if serde_json::to_vec(value)
        .map_err(|_| error(ErrorCode::InvalidRequest, "Invalid JSON value"))?
        .len()
        > MAX_VALUE_BYTES
    {
        return Err(error(
            ErrorCode::InvalidRequest,
            "Row JSON exceeds the value budget",
        ));
    }
    if table.definition.kind == TableKind::Context {
        return Ok(());
    }
    let object = value
        .as_object()
        .ok_or_else(|| error(ErrorCode::InvalidRequest, "Table rows must be JSON objects"))?;
    for field in &table.definition.fields {
        let Some(value) = object.get(&field.name) else {
            if field.required {
                return Err(error(ErrorCode::InvalidRequest, "Required field missing"));
            }
            continue;
        };
        let valid = match field.kind {
            FieldType::String => value.is_string(),
            FieldType::Number => value.is_number(),
            FieldType::Boolean => value.is_boolean(),
            FieldType::Object => value.is_object(),
            FieldType::Array => value.is_array(),
            FieldType::Any => true,
        };
        if !valid {
            return Err(error(
                ErrorCode::InvalidRequest,
                "Field value does not match its declared type",
            ));
        }
    }
    Ok(())
}
fn next_revision(table: &mut Table) -> Result<u64> {
    table.data_revision = table
        .data_revision
        .checked_add(1)
        .filter(|v| *v <= 9_007_199_254_740_991)
        .ok_or_else(|| error(ErrorCode::Conflict, "Data revision exhausted"))?;
    Ok(table.data_revision)
}
fn expiry(now: u64, ttl: Option<u64>) -> Result<Option<u64>> {
    ttl.map(|ttl| {
        if ttl == 0 {
            Err(error(ErrorCode::InvalidRequest, "TTL must be positive"))
        } else {
            now.checked_add(ttl)
                .filter(|v| *v <= 9_007_199_254_740_991)
                .ok_or_else(|| error(ErrorCode::InvalidRequest, "TTL exceeds timestamp range"))
        }
    })
    .transpose()
}
fn matches(row: &Row, filter: &Filter) -> bool {
    let value = row.value.pointer(&filter.pointer);
    match filter.operator {
        Operator::Exists => value.is_some(),
        Operator::Eq => value == Some(&filter.value),
        Operator::Ne => value != Some(&filter.value),
        Operator::Gt | Operator::Lt => {
            let order = match (value, &filter.value) {
                (Some(Value::Number(a)), Value::Number(b)) => {
                    if let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) {
                        Some(a.cmp(&b))
                    } else if let (Some(a), Some(b)) = (a.as_u64(), b.as_u64()) {
                        Some(a.cmp(&b))
                    } else if a.as_i64().is_some_and(|v| v < 0) && b.as_u64().is_some() {
                        Some(std::cmp::Ordering::Less)
                    } else if b.as_i64().is_some_and(|v| v < 0) && a.as_u64().is_some() {
                        Some(std::cmp::Ordering::Greater)
                    } else {
                        a.as_f64()
                            .zip(b.as_f64())
                            .and_then(|(a, b)| a.partial_cmp(&b))
                    }
                }
                (Some(Value::String(a)), Value::String(b)) => Some(a.cmp(b)),
                _ => None,
            };
            order.is_some_and(|v| {
                if matches!(filter.operator, Operator::Gt) {
                    v.is_gt()
                } else {
                    v.is_lt()
                }
            })
        }
    }
}
/// Apply exactly once when a receipt is supplied. Mutations always require one.
/// The fingerprint includes table, command and TTL; each Store is namespace-scoped. Conflicting reuse
/// fails before any reads or writes of application data.
pub fn apply(
    store: &mut impl Store,
    command: Command,
    receipt_key: Option<&str>,
    now: u64,
) -> Result<Value> {
    if !valid_id(command.table_id()) {
        return Err(error(ErrorCode::InvalidRequest, "Invalid table ID"));
    }
    if command.mutates() && receipt_key.is_none() {
        return Err(error(
            ErrorCode::InvalidRequest,
            "Mutations require an idempotency receipt key",
        ));
    }
    if receipt_key
        .is_some_and(|key| key.is_empty() || key.len() > 256 || key.chars().any(char::is_control))
    {
        return Err(error(ErrorCode::InvalidRequest, "Invalid receipt key"));
    }
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&command)
                .map_err(|_| error(ErrorCode::InvalidRequest, "Invalid command"))?
        )
    );
    if let Some(key) = receipt_key {
        if let Some(receipt) = store.receipt(key)? {
            return if receipt.fingerprint == fingerprint {
                Ok(receipt.result)
            } else {
                Err(error(
                    ErrorCode::Conflict,
                    "Receipt key was already used for a different command",
                ))
            };
        }
    }
    let result = execute(store, command, now)?;
    if let Some(key) = receipt_key {
        store.save_receipt(Receipt {
            key: key.into(),
            fingerprint,
            result: result.clone(),
            completed_at: Some(now),
        })?;
    }
    Ok(result)
}
/// One receipt and one host transaction cover the entire batch. Errors, including
/// output-budget errors after staged writes, must roll back all application data.
pub fn apply_batch(
    store: &mut impl Store,
    commands: Vec<Command>,
    receipt_key: &str,
    now: u64,
) -> Result<Value> {
    if commands.is_empty()
        || commands.len() > 100
        || receipt_key.is_empty()
        || receipt_key.len() > 256
        || receipt_key.chars().any(char::is_control)
        || commands.iter().any(|c| !valid_id(c.table_id()))
    {
        return Err(error(
            ErrorCode::InvalidRequest,
            "Invalid bounded batch or receipt key",
        ));
    }
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&commands)
                .map_err(|_| error(ErrorCode::InvalidRequest, "Invalid batch"))?
        )
    );
    if let Some(receipt) = store.receipt(receipt_key)? {
        return if receipt.fingerprint == fingerprint {
            Ok(receipt.result)
        } else {
            Err(error(
                ErrorCode::Conflict,
                "Receipt key was already used for a different command",
            ))
        };
    }
    let mut results = Vec::new();
    for command in commands {
        results.push(execute(store, command, now)?);
    }
    let result = json!({"results":results});
    if result.to_string().len() > MAX_QUERY_BYTES {
        return Err(error(
            ErrorCode::InvalidRequest,
            "Batch output exceeds the result budget",
        ));
    }
    store.save_receipt(Receipt {
        key: receipt_key.into(),
        fingerprint,
        result: result.clone(),
        completed_at: Some(now),
    })?;
    Ok(result)
}
fn execute(store: &mut impl Store, command: Command, now: u64) -> Result<Value> {
    if let Command::CreateTable { definition } = command {
        validate_definition(&definition)?;
        if let Some(existing) = store.table(&definition.id)? {
            return if existing.definition == definition {
                Ok(json!({"status":"existing","table":existing}))
            } else {
                Ok(json!({"status":"conflict","table":existing}))
            };
        }
        let table = Table {
            definition,
            version: 1,
            data_revision: 0,
        };
        store.save_table(table.clone())?;
        return Ok(json!({"status":"created","table":table}));
    }
    let mut table = store
        .table(command.table_id())?
        .ok_or_else(|| error(ErrorCode::NotFound, "Table not found"))?;
    let id = table.definition.id.clone();
    match command {
        Command::GetTable { .. } => Ok(json!({"table":table})),
        Command::RenameTable {
            name,
            expected_version,
            ..
        } => {
            if !valid_name(&name) {
                return Err(error(ErrorCode::InvalidRequest, "Invalid table name"));
            }
            if table.version != expected_version {
                return Ok(json!({"status":"conflict","table":table}));
            }
            if table.definition.name != name {
                table.definition.name = name;
                table.version = table
                    .version
                    .checked_add(1)
                    .ok_or_else(|| error(ErrorCode::Conflict, "Table version exhausted"))?;
                store.save_table(table.clone())?;
            }
            Ok(json!({"status":"updated","table":table}))
        }
        Command::Get { key, .. } => {
            if !valid_key(&key) {
                return Err(error(ErrorCode::InvalidRequest, "Invalid row key"));
            }
            let row = store.row(&id, &key)?.filter(|r| r.live(now));
            Ok(json!({"status":if row.is_some(){"found"}else{"not_found"},"row":row}))
        }
        Command::InsertIfAbsent {
            key, value, ttl_ms, ..
        } => {
            if !valid_key(&key) {
                return Err(error(ErrorCode::InvalidRequest, "Invalid row key"));
            }
            validate_value(&table, &value)?;
            if let Some(row) = store.row(&id, &key)?.filter(|r| r.live(now)) {
                return Ok(json!({"status":"existing","inserted":false,"row":row}));
            }
            let row = Row {
                key,
                value,
                version: next_revision(&mut table)?,
                expires_at: expiry(now, ttl_ms)?,
            };
            store.save_row(&id, row.clone())?;
            store.save_table(table)?;
            Ok(json!({"status":"inserted","inserted":true,"row":row}))
        }
        Command::Put {
            key,
            value,
            expected_version,
            ttl_ms,
            ..
        } => {
            if !valid_key(&key) {
                return Err(error(ErrorCode::InvalidRequest, "Invalid row key"));
            }
            validate_value(&table, &value)?;
            let previous = store.row(&id, &key)?.filter(|r| r.live(now));
            if expected_version
                .is_some_and(|version| previous.as_ref().map(|r| r.version) != Some(version))
            {
                return Ok(json!({"status":"conflict","row":previous}));
            }
            let row = Row {
                key,
                value,
                version: next_revision(&mut table)?,
                expires_at: expiry(now, ttl_ms)?,
            };
            store.save_row(&id, row.clone())?;
            store.save_table(table)?;
            Ok(json!({"status":"stored","row":row}))
        }
        Command::Patch {
            key,
            patch,
            remove_fields,
            expected_version,
            ..
        } => {
            if !valid_key(&key) || remove_fields.len() > 100 {
                return Err(error(ErrorCode::InvalidRequest, "Invalid row patch"));
            }
            let previous = store.row(&id, &key)?.filter(|row| row.live(now));
            if expected_version
                .is_some_and(|version| previous.as_ref().map(|row| row.version) != Some(version))
            {
                return Ok(json!({"status":"conflict","row":previous}));
            }
            let Some(mut row) = previous else {
                return Ok(json!({"status":"not_found","row":Value::Null}));
            };
            let mut value = row.value.clone();
            let object = value.as_object_mut().ok_or_else(|| {
                error(
                    ErrorCode::InvalidRequest,
                    "Partial updates require an object value",
                )
            })?;
            for field in remove_fields {
                object.remove(&field);
            }
            for (field, value) in patch {
                object.insert(field, value);
            }
            validate_value(&table, &value)?;
            if value == row.value {
                return Ok(json!({"status":"unchanged","row":row}));
            }
            row.value = value;
            row.version = next_revision(&mut table)?;
            store.save_row(&id, row.clone())?;
            store.save_table(table)?;
            Ok(json!({"status":"stored","row":row}))
        }
        Command::Delete {
            key,
            expected_version,
            ..
        } => {
            if !valid_key(&key) {
                return Err(error(ErrorCode::InvalidRequest, "Invalid row key"));
            }
            let previous = store.row(&id, &key)?.filter(|r| r.live(now));
            if expected_version
                .is_some_and(|version| previous.as_ref().map(|r| r.version) != Some(version))
            {
                return Ok(json!({"status":"conflict","row":previous}));
            }
            if previous.is_some() {
                next_revision(&mut table)?;
                store.remove_row(&id, &key)?;
                store.save_table(table)?;
            }
            Ok(
                json!({"status":if previous.is_some(){"deleted"}else{"not_found"},"deleted":previous.is_some()}),
            )
        }
        Command::Query { query, .. } => {
            if !(1..=MAX_PAGE_SIZE).contains(&query.limit)
                || query.filters.len() > 16
                || query.cursor.as_ref().is_some_and(|s| !valid_key(s))
                || query.filters.iter().any(|f| {
                    f.pointer.len() > 256 || (!f.pointer.is_empty() && !f.pointer.starts_with('/'))
                })
            {
                return Err(error(ErrorCode::InvalidRequest, "Invalid bounded query"));
            }
            let page = store.scan(&id, query.cursor.as_deref(), MAX_SCAN_SIZE)?;
            let source = page.rows;
            let mut rows = Vec::new();
            let mut cursor = None;
            let mut scanned = 0;
            let mut bytes = 0;
            for (index, row) in source.iter().take(MAX_SCAN_SIZE).enumerate() {
                if row.live(now) && query.filters.iter().all(|filter| matches(row, filter)) {
                    let size = serde_json::to_vec(row)
                        .map_err(|_| error(ErrorCode::Storage, "Invalid stored row"))?
                        .len();
                    if bytes + size > MAX_QUERY_BYTES {
                        cursor = if index > 0 {
                            Some(source[index - 1].key.clone())
                        } else {
                            query.cursor.clone()
                        };
                        break;
                    }
                    bytes += size;
                    rows.push(row.clone());
                }
                scanned = index + 1;
                cursor = if index + 1 < source.len() || page.more {
                    Some(row.key.clone())
                } else {
                    None
                };
                if rows.len() >= query.limit {
                    break;
                }
            }
            Ok(json!({"rows":rows,"nextCursor":cursor,"scanned":scanned}))
        }
        Command::CreateTable { .. } => unreachable!(),
    }
}
/// Native reference store and simulation-overlay building block. Persistent hosts
/// implement Store over SQLite rather than serializing this map into checkpoints.
#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    pub tables: BTreeMap<String, Table>,
    pub rows: BTreeMap<(String, String), Row>,
    pub receipts: BTreeMap<String, Receipt>,
}
impl MemoryStore {
    pub fn execute_batch(
        &mut self,
        commands: Vec<Command>,
        receipt: &str,
        now: u64,
    ) -> Result<Value> {
        let before = self.clone();
        let result = apply_batch(self, commands, receipt, now);
        if result.is_err() {
            *self = before;
        }
        result
    }
    pub fn execute(&mut self, command: Command, receipt: Option<&str>, now: u64) -> Result<Value> {
        let before = self.clone();
        let result = apply(self, command, receipt, now);
        if result.is_err() {
            *self = before;
        }
        result
    }
}
impl Store for MemoryStore {
    fn table(&mut self, id: &str) -> Result<Option<Table>> {
        Ok(self.tables.get(id).cloned())
    }
    fn save_table(&mut self, table: Table) -> Result<()> {
        self.tables.insert(table.definition.id.clone(), table);
        Ok(())
    }
    fn row(&mut self, table: &str, key: &str) -> Result<Option<Row>> {
        Ok(self.rows.get(&(table.into(), key.into())).cloned())
    }
    fn save_row(&mut self, table: &str, row: Row) -> Result<()> {
        self.rows.insert((table.into(), row.key.clone()), row);
        Ok(())
    }
    fn remove_row(&mut self, table: &str, key: &str) -> Result<()> {
        self.rows.remove(&(table.into(), key.into()));
        Ok(())
    }
    fn scan(&mut self, table: &str, after: Option<&str>, limit: usize) -> Result<ScanPage> {
        let mut rows = Vec::new();
        let mut bytes = 0;
        let mut more = false;
        for (_, row) in self
            .rows
            .iter()
            .filter(|((id, key), _)| id == table && after.is_none_or(|after| key.as_str() > after))
        {
            let size = serde_json::to_vec(row)
                .map_err(|_| error(ErrorCode::Storage, "Invalid stored row"))?
                .len();
            if size > MAX_SCAN_BYTES {
                return Err(error(ErrorCode::Storage, "Stored row exceeds scan budget"));
            }
            if rows.len() >= limit || bytes + size > MAX_SCAN_BYTES {
                more = true;
                break;
            }
            rows.push(row.clone());
            bytes += size;
        }
        Ok(ScanPage { rows, more })
    }
    fn receipt(&mut self, key: &str) -> Result<Option<Receipt>> {
        Ok(self.receipts.get(key).cloned())
    }
    fn save_receipt(&mut self, receipt: Receipt) -> Result<()> {
        self.receipts.insert(receipt.key.clone(), receipt);
        Ok(())
    }
}
