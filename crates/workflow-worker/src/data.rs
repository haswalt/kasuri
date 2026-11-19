//! Workspace-local SQLite data, isolated from workflow/run checkpoints.
use crate::{WorkflowWorkspace, api_error, internal, now};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};
use worker::*;
use workflow_data::{
    Command, DataError, ErrorCode, MAX_SCAN_BYTES, Receipt, Row, ScanPage, Store, Table,
};
use workflow_service::Workspace;

const TABLES: &str = "CREATE TABLE IF NOT EXISTS data_tables (scope TEXT NOT NULL,id TEXT NOT NULL,body TEXT NOT NULL,PRIMARY KEY(scope,id))";
const ROWS: &str = "CREATE TABLE IF NOT EXISTS data_rows (scope TEXT NOT NULL,table_id TEXT NOT NULL,key TEXT NOT NULL,body TEXT NOT NULL,PRIMARY KEY(scope,table_id,key))";
const RECEIPTS: &str = "CREATE TABLE IF NOT EXISTS data_receipts (scope TEXT NOT NULL,domain TEXT NOT NULL,key TEXT NOT NULL,body TEXT NOT NULL,run_id TEXT,expires_at INTEGER,PRIMARY KEY(scope,domain,key))";
fn storage_error(_: impl std::fmt::Display) -> DataError {
    workflow_data::error(ErrorCode::Storage, "Data storage operation failed")
}
fn invalid(message: &str) -> DataError {
    workflow_data::error(ErrorCode::InvalidRequest, message)
}
#[derive(Deserialize)]
struct Stored {
    body: String,
}
fn decode<T: DeserializeOwned>(text: &str) -> workflow_data::Result<T> {
    if text.len() > 131_072 {
        return Err(storage_error("Stored data exceeds budget"));
    }
    serde_json::from_str(text).map_err(storage_error)
}
/// Scope and receipt domain are supplied by the host, never by untrusted commands.
pub(crate) struct SqlDataStore {
    sql: SqlStorage,
    scope: String,
    domain: String,
    run_id: Option<String>,
    time: u64,
}
impl SqlDataStore {
    pub(crate) fn new(
        sql: SqlStorage,
        scope: String,
        domain: String,
        run_id: Option<String>,
        time: u64,
    ) -> workflow_data::Result<Self> {
        sql.exec("CREATE TABLE IF NOT EXISTS data_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL)",None).map_err(storage_error)?;
        sql.exec(
            "INSERT OR IGNORE INTO data_schema(id,version) VALUES(1,1)",
            None,
        )
        .map_err(storage_error)?;
        #[derive(Deserialize)]
        struct Schema {
            version: u32,
        }
        let schema = sql
            .exec("SELECT version FROM data_schema WHERE id=1", None)
            .map_err(storage_error)?
            .one::<Schema>()
            .map_err(storage_error)?;
        if schema.version != 1 {
            return Err(storage_error("Unsupported data schema version"));
        }
        sql.exec(TABLES, None).map_err(storage_error)?;
        sql.exec(ROWS, None).map_err(storage_error)?;
        sql.exec(RECEIPTS, None).map_err(storage_error)?;
        sql.exec(
            "CREATE INDEX IF NOT EXISTS data_receipt_expiry ON data_receipts(scope,expires_at)",
            None,
        )
        .map_err(storage_error)?;
        sql.exec("DELETE FROM data_receipts WHERE rowid IN (SELECT rowid FROM data_receipts WHERE scope=? AND expires_at<=? LIMIT 100)",Some(vec![scope.clone().into(),(time as i64).into()])).map_err(storage_error)?;
        Ok(Self {
            sql,
            scope,
            domain,
            run_id,
            time,
        })
    }
    fn read<T: DeserializeOwned>(
        &self,
        query: &str,
        bindings: Vec<SqlStorageValue>,
    ) -> workflow_data::Result<Option<T>> {
        let rows = self
            .sql
            .exec(query, Some(bindings))
            .map_err(storage_error)?
            .to_array::<Stored>()
            .map_err(storage_error)?;
        rows.first().map(|r| decode(&r.body)).transpose()
    }
    pub(crate) fn tables(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> workflow_data::Result<Value> {
        if !(1..=100).contains(&limit) || cursor.is_some_and(|v| v.len() > 64) {
            return Err(invalid("Invalid table page"));
        }
        let rows = self
            .sql
            .exec(
                "SELECT body FROM data_tables WHERE scope=? AND id>? ORDER BY id LIMIT ?",
                Some(vec![
                    self.scope.clone().into(),
                    cursor.unwrap_or("").into(),
                    ((limit + 1) as i64).into(),
                ]),
            )
            .map_err(storage_error)?
            .to_array::<Stored>()
            .map_err(storage_error)?;
        let more = rows.len() > limit;
        let tables: Vec<Table> = rows
            .iter()
            .take(limit)
            .map(|r| decode(&r.body))
            .collect::<workflow_data::Result<_>>()?;
        let next = if more {
            tables.last().map(|t| t.definition.id.clone())
        } else {
            None
        };
        let summaries:Vec<_>=tables.iter().map(|table|json!({"id":table.definition.id,"name":table.definition.name,"kind":table.definition.kind,"fieldCount":table.definition.fields.len(),"version":table.version,"dataRevision":table.data_revision})).collect();
        Ok(json!({"tables":summaries,"nextCursor":next}))
    }
}
impl Store for SqlDataStore {
    fn table(&mut self, id: &str) -> workflow_data::Result<Option<Table>> {
        self.read(
            "SELECT body FROM data_tables WHERE scope=? AND id=?",
            vec![self.scope.clone().into(), id.into()],
        )
    }
    fn save_table(&mut self, table: Table) -> workflow_data::Result<()> {
        self.sql.exec("INSERT INTO data_tables(scope,id,body) VALUES(?,?,?) ON CONFLICT(scope,id) DO UPDATE SET body=excluded.body",Some(vec![self.scope.clone().into(),table.definition.id.clone().into(),serde_json::to_string(&table).map_err(storage_error)?.into()])).map_err(storage_error)?;
        Ok(())
    }
    fn row(&mut self, table: &str, key: &str) -> workflow_data::Result<Option<Row>> {
        self.read(
            "SELECT body FROM data_rows WHERE scope=? AND table_id=? AND key=?",
            vec![self.scope.clone().into(), table.into(), key.into()],
        )
    }
    fn save_row(&mut self, table: &str, row: Row) -> workflow_data::Result<()> {
        self.sql.exec("INSERT INTO data_rows(scope,table_id,key,body) VALUES(?,?,?,?) ON CONFLICT(scope,table_id,key) DO UPDATE SET body=excluded.body",Some(vec![self.scope.clone().into(),table.into(),row.key.clone().into(),serde_json::to_string(&row).map_err(storage_error)?.into()])).map_err(storage_error)?;
        Ok(())
    }
    fn remove_row(&mut self, table: &str, key: &str) -> workflow_data::Result<()> {
        self.sql
            .exec(
                "DELETE FROM data_rows WHERE scope=? AND table_id=? AND key=?",
                Some(vec![self.scope.clone().into(), table.into(), key.into()]),
            )
            .map_err(storage_error)?;
        Ok(())
    }
    fn scan(
        &mut self,
        table: &str,
        after: Option<&str>,
        limit: usize,
    ) -> workflow_data::Result<ScanPage> {
        let cursor=self.sql.exec("SELECT body FROM data_rows WHERE scope=? AND table_id=? AND key>? ORDER BY key LIMIT ?",Some(vec![self.scope.clone().into(),table.into(),after.unwrap_or("").into(),((limit+1)as i64).into()])).map_err(storage_error)?;
        let mut rows = Vec::new();
        let mut bytes = 0;
        let mut more = false;
        for stored in cursor.next::<Stored>() {
            let stored = stored.map_err(storage_error)?;
            if stored.body.len() > MAX_SCAN_BYTES {
                return Err(storage_error("Stored row exceeds scan budget"));
            }
            if rows.len() >= limit || bytes + stored.body.len() > MAX_SCAN_BYTES {
                more = true;
                break;
            }
            let row = decode(&stored.body)?;
            bytes += stored.body.len();
            rows.push(row);
        }
        Ok(ScanPage { rows, more })
    }
    fn receipt(&mut self, key: &str) -> workflow_data::Result<Option<Receipt>> {
        self.read("SELECT body FROM data_receipts WHERE scope=? AND domain=? AND key=? AND (expires_at IS NULL OR expires_at>?)",vec![self.scope.clone().into(),self.domain.clone().into(),key.into(),(self.time as i64).into()])
    }
    fn save_receipt(&mut self, receipt: Receipt) -> workflow_data::Result<()> {
        let expiry = if self.run_id.is_some() {
            SqlStorageValue::Null
        } else {
            (self.time.saturating_add(86_400_000) as i64).into()
        };
        self.sql.exec("INSERT INTO data_receipts(scope,domain,key,body,run_id,expires_at) VALUES(?,?,?,?,?,?) ON CONFLICT(scope,domain,key) DO UPDATE SET body=excluded.body,run_id=excluded.run_id,expires_at=excluded.expires_at",Some(vec![self.scope.clone().into(),self.domain.clone().into(),receipt.key.clone().into(),serde_json::to_string(&receipt).map_err(storage_error)?.into(),self.run_id.clone().map(SqlStorageValue::String).unwrap_or(SqlStorageValue::Null),expiry])).map_err(storage_error)?;
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequestBody {
    command: Option<Command>,
    commands: Option<Vec<Command>>,
    idempotency_key: Option<String>,
}
fn status(error: &DataError) -> u16 {
    match error.code {
        ErrorCode::InvalidRequest => 400,
        ErrorCode::NotFound => 404,
        ErrorCode::Conflict => 409,
        ErrorCode::Storage => 500,
    }
}
pub(crate) struct DataExecution {
    pub result: Value,
    pub replayed: bool,
    pub workspace: Option<Workspace>,
}
pub(crate) struct McpDataAccess<'a> {
    pub owner: &'a WorkflowWorkspace,
}
impl workflow_mcp::DataAccess for McpDataAccess<'_> {
    fn tables(
        &mut self,
        cursor: Option<&str>,
        limit: usize,
        time: u64,
    ) -> workflow_data::Result<Value> {
        let cursor = cursor.map(str::to_string);
        self.owner
            .data_transaction("mcp-read".into(), time, move |store| {
                store.tables(cursor.as_deref(), limit)
            })
    }
    fn execute(
        &mut self,
        workspace: &mut Workspace,
        commands: Vec<Command>,
        batch: bool,
        receipt: Option<&str>,
        actor: &str,
        time: u64,
    ) -> workflow_data::Result<Value> {
        let snapshot = commands
            .iter()
            .any(Command::mutates)
            .then(|| workspace.clone());
        let execution = self.owner.execute_data(
            commands,
            batch,
            receipt.map(str::to_string),
            actor.into(),
            format!("mcp:{actor}"),
            snapshot,
            time,
        )?;
        if let Some(updated) = execution.workspace {
            *workspace = updated;
        }
        Ok(execution.result)
    }
}
impl WorkflowWorkspace {
    fn data_transaction<T: 'static>(
        &self,
        domain: String,
        time: u64,
        operation: impl FnOnce(&mut SqlDataStore) -> workflow_data::Result<T> + 'static,
    ) -> workflow_data::Result<T> {
        let failure = Rc::new(RefCell::new(None));
        let failed = failure.clone();
        let result = self.sql_transaction(move |sql| {
            let result = SqlDataStore::new(sql, "live".into(), domain, None, time)
                .and_then(|mut store| operation(&mut store));
            result.map_err(|error| {
                *failed.borrow_mut() = Some(error);
                internal("Data transaction aborted")
            })
        });
        match result {
            Ok(value) => Ok(value),
            Err(error) => Err(failure
                .borrow_mut()
                .take()
                .unwrap_or_else(|| storage_error(error))),
        }
    }
    fn execute_data(
        &self,
        commands: Vec<Command>,
        batch: bool,
        receipt: Option<String>,
        actor: String,
        domain: String,
        mut workspace: Option<Workspace>,
        time: u64,
    ) -> workflow_data::Result<DataExecution> {
        if commands.is_empty() || commands.len() > 100 || (!batch && commands.len() != 1) {
            return Err(invalid(
                "Provide 1–100 commands, or exactly one non-batch command",
            ));
        }
        self.data_transaction(domain,time,move |store| {
            let replayed=if let Some(key)=receipt.as_deref(){store.receipt(key)?.is_some()}else{false};
            let ids:BTreeSet<String>=commands.iter().map(|c|c.table_id().to_string()).collect();
            let before:Vec<_>=if workspace.is_some(){ids.iter().map(|id|store.table(id)).collect::<workflow_data::Result<_>>()?}else{vec![]};
            let operations:Vec<_>=commands.iter().map(|command|match command {Command::CreateTable{..}=>"create table",Command::RenameTable{..}=>"rename table",Command::InsertIfAbsent{..}=>"insert if absent",Command::Put{..}=>"store row",Command::Patch{..}=>"patch row",Command::Delete{..}=>"delete row",Command::Get{..}|Command::GetTable{..}|Command::Query{..}=>"read"}).collect();
            let result=if batch {workflow_data::apply_batch(store,commands,receipt.as_deref().ok_or_else(||invalid("Batches require an idempotencyKey"))?,time)?}else{workflow_data::apply(store,commands.into_iter().next().unwrap(),receipt.as_deref(),time)?};
            let after:Vec<_>=if workspace.is_some(){ids.iter().map(|id|store.table(id)).collect::<workflow_data::Result<_>>()?}else{vec![]};
            if !replayed&&before!=after&&let Some(workspace)=&mut workspace {
                workspace.record_audit(&actor,"data.changed",ids.iter().next().map(String::as_str).unwrap_or(""),"succeeded",time,"");
                if let Some(event)=workspace.audit.last_mut(){
                    let name=after.first().and_then(|t|t.as_ref()).map(|t|t.definition.name.as_str()).unwrap_or("Data tables");
                    event["summary"]=json!(format!("{name}: {} operation(s)",operations.len()));
                    event["entity"]=json!({"type":"data_table","id":ids.iter().next(),"name":name});
                    event["context"]=json!({"tableIds":ids,"operations":operations});
                    event["before"]=json!(before.iter().map(|t|t.as_ref().map(|t|json!({"id":t.definition.id,"version":t.version,"dataRevision":t.data_revision}))).collect::<Vec<_>>());
                    event["after"]=json!(after.iter().map(|t|t.as_ref().map(|t|json!({"id":t.definition.id,"version":t.version,"dataRevision":t.data_revision}))).collect::<Vec<_>>());
                }
                crate::storage::write_workspace_records(&store.sql,workspace).map_err(storage_error)?;
            }
            Ok(DataExecution{result,replayed,workspace})
        })
    }
    pub(crate) async fn data_management(
        &self,
        method: Method,
        url: &Url,
        bytes: &[u8],
    ) -> Result<Option<Response>> {
        let path = url.path();
        if path != "/api/data" && path != "/api/tables" {
            return Ok(None);
        }
        let failure = Rc::new(RefCell::new(None::<DataError>));
        let failed = failure.clone();
        let time = now();
        if path == "/api/tables" && method == Method::Get {
            let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            let limit = match query.get("limit").map(|v| v.parse::<usize>()).transpose() {
                Ok(limit) => limit.unwrap_or(25),
                Err(_) => return Ok(Some(api_error(400, "Invalid table limit")?)),
            };
            let cursor = query.get("cursor").cloned();
            let result = self.sql_transaction(move |sql| {
                let result = SqlDataStore::new(sql, "live".into(), "api".into(), None, time)
                    .and_then(|store| store.tables(cursor.as_deref(), limit));
                result.map_err(|error| {
                    *failed.borrow_mut() = Some(error);
                    internal("Data read aborted")
                })
            });
            return match result {
                Ok(value) => Ok(Some(Response::from_json(&value)?)),
                Err(error) => {
                    if let Some(failure) = failure.borrow_mut().take() {
                        Ok(Some(api_error(status(&failure), &failure.message)?))
                    } else {
                        Err(error)
                    }
                }
            };
        }
        if path != "/api/data" || method != Method::Post {
            return Ok(Some(api_error(405, "Unsupported data operation")?));
        }
        let input: RequestBody = match serde_json::from_slice(bytes) {
            Ok(input) => input,
            Err(_) => {
                return Ok(Some(api_error(
                    400,
                    "Expected one command or commands and an optional idempotencyKey",
                )?));
            }
        };
        let (commands, batch) = match (input.command, input.commands) {
            (Some(command), None) => (vec![command], false),
            (None, Some(commands)) if !commands.is_empty() => (commands, true),
            _ => {
                return Ok(Some(api_error(
                    400,
                    "Provide exactly one command or a non-empty commands array",
                )?));
            }
        };
        if input
            .idempotency_key
            .as_ref()
            .is_some_and(|key| !(8..=128).contains(&key.len()))
        {
            return Ok(Some(api_error(
                400,
                "Use an 8–128 character idempotencyKey",
            )?));
        }
        let workspace = if commands.iter().any(Command::mutates) {
            Some(self.load()?)
        } else {
            None
        };
        match self.execute_data(
            commands,
            batch,
            input.idempotency_key,
            "browser".into(),
            "api".into(),
            workspace,
            time,
        ) {
            Ok(execution) => Ok(Some(Response::from_json(
                &json!({"result":execution.result,"replayed":execution.replayed}),
            )?)),
            Err(error) => Ok(Some(api_error(status(&error), &error.message)?)),
        }
    }
}
