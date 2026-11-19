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
    fn data_transfer(&self, method: Method, url: &Url, bytes: &[u8]) -> Result<Response> {
        use workflow_data::transfer::{self, ImportMode, ImportRow};
        let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
        if parts.len() != 4 || !matches!(parts[3], "import" | "export") {
            return api_error(404, "Data transfer not found");
        }
        let table_id = parts[2].to_string();
        if table_id.is_empty()
            || table_id.len() > 64
            || !table_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return api_error(400, "Invalid table ID");
        }
        let time = now();
        let result = if parts[3] == "export" && method == Method::Get {
            let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            let format = query.get("format").cloned().unwrap_or("json".into());
            let cursor = query.get("cursor").cloned();
            let expected = match query
                .get("dataRevision")
                .map(|v| v.parse::<u64>())
                .transpose()
            {
                Ok(value) => value,
                Err(_) => return api_error(400, "Invalid data revision"),
            };
            self.data_transaction("api-export".into(),time,move |store|{
                let table=store.table(&table_id)?.ok_or_else(||workflow_data::error(ErrorCode::NotFound,"Table not found"))?;
                if expected.is_some_and(|v|v!=table.data_revision){return Err(workflow_data::error(ErrorCode::Conflict,"Data changed during export; restart the export"))}
                let page=workflow_data::apply(store,Command::Query{table_id:table_id.clone(),query:workflow_data::Query{cursor,limit:100,filters:vec![]}},None,time)?;
                let rows:Vec<Row>=serde_json::from_value(page["rows"].clone()).map_err(storage_error)?;
                let rows:Vec<ImportRow>=rows.into_iter().map(|r|ImportRow{key:r.key,value:r.value,expires_at:r.expires_at}).collect();
                let content=transfer::encode(&format,&rows)?;
                Ok(json!({"tableId":table_id,"format":format,"content":content,"count":rows.len(),"nextCursor":page["nextCursor"],"dataRevision":table.data_revision,"tableVersion":table.version}))
            })
        } else if parts[3] == "import" && method == Method::Post {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Input {
                format: String,
                content: String,
                #[serde(default)]
                mode: ImportMode,
                expected_data_revision: u64,
                #[serde(default)]
                commit: bool,
                #[serde(default)]
                idempotency_key: Option<String>,
            }
            let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            let input: Input = if let Some(format) = query.get("format") {
                let content = match String::from_utf8(bytes.to_vec()) {
                    Ok(text) => text,
                    Err(_) => return api_error(400, "Import file must be UTF-8"),
                };
                let expected_data_revision = match query
                    .get("expectedDataRevision")
                    .and_then(|v| v.parse::<u64>().ok())
                {
                    Some(v) => v,
                    None => return api_error(400, "expectedDataRevision is required"),
                };
                let mode = match query.get("mode").map(String::as_str).unwrap_or("insert") {
                    "insert" => ImportMode::Insert,
                    "upsert" => ImportMode::Upsert,
                    _ => return api_error(400, "Invalid import mode"),
                };
                let commit = match query.get("commit").map(String::as_str).unwrap_or("false") {
                    "true" => true,
                    "false" => false,
                    _ => return api_error(400, "Invalid import commit flag"),
                };
                Input {
                    format: format.clone(),
                    content,
                    expected_data_revision,
                    mode,
                    commit,
                    idempotency_key: query.get("idempotencyKey").cloned(),
                }
            } else {
                match serde_json::from_slice(bytes) {
                    Ok(input) => input,
                    Err(_) => {
                        return api_error(
                            400,
                            "Expected format, content, mode and expectedDataRevision",
                        );
                    }
                }
            };
            let rows = match transfer::parse(&input.format, &input.content) {
                Ok(rows) => rows,
                Err(error) => return api_error(status(&error), &error.message),
            };
            if !input.commit {
                self.data_transaction("api-import-preview".into(),time,move |store|{
                    let table=store.table(&table_id)?.ok_or_else(||workflow_data::error(ErrorCode::NotFound,"Table not found"))?;
                    let skipped=transfer::preflight(store,&table,&rows,&input.mode,Some(input.expected_data_revision),time)?;
                    Ok(json!({"status":"preview","rows":rows.len(),"skippedExpired":skipped,"dataRevision":table.data_revision}))
                })
            } else {
                if input
                    .idempotency_key
                    .as_ref()
                    .is_none_or(|v| !(8..=128).contains(&v.len()))
                {
                    return api_error(
                        400,
                        "Import commit requires an 8-128 character idempotencyKey",
                    );
                }
                self.execute_data(
                    vec![Command::ImportRows {
                        table_id,
                        rows,
                        mode: input.mode,
                        expected_data_revision: Some(input.expected_data_revision),
                    }],
                    false,
                    input.idempotency_key,
                    "browser".into(),
                    "api-import".into(),
                    Some(self.load()?),
                    time,
                )
                .map(|execution| execution.result)
            }
        } else {
            return api_error(405, "Unsupported data transfer method");
        };
        match result {
            Ok(value) => {
                let mut response = Response::from_json(&value)?;
                response.headers_mut().set("Cache-Control", "no-store")?;
                Ok(response)
            }
            Err(error) => api_error(status(&error), &error.message),
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
            let operations:Vec<_>=commands.iter().map(|command|match command {Command::CreateTable{..}=>"create table",Command::RenameTable{..}=>"rename table",Command::InsertIfAbsent{..}=>"insert if absent",Command::Put{..}=>"store row",Command::Patch{..}=>"patch row",Command::Delete{..}=>"delete row",Command::Increment{..}=>"increment counter",Command::ImportRows{..}=>"import rows",Command::Get{..}|Command::GetTable{..}|Command::Query{..}=>"read"}).collect();
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
        if path.starts_with("/api/tables/") {
            return self.data_transfer(method, url, bytes).map(Some);
        }
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

/// Snapshot and receipt lifecycle is committed with the corresponding checkpoint.
/// SQL copies stay outside Rust heaps and run JSON; absent source tables remain
/// absent until an explicit create command executes in the simulation.
pub(crate) fn sync_run_data(sql: &SqlStorage, workspace: &Workspace, time: u64) -> Result<()> {
    let active = workspace.data_runs().map_err(internal)?;
    if active.is_empty() {
        // Avoid creating data tables on installations that have never used them.
        #[derive(Deserialize)]
        struct Found {
            count: u32,
        }
        if sql.exec("SELECT count(*) AS count FROM sqlite_master WHERE type='table' AND name='data_simulations'",None)?.one::<Found>()?.count == 0 {return Ok(())}
    }
    SqlDataStore::new(sql.clone(), "live".into(), "engine".into(), None, time).map_err(internal)?;
    sql.exec(
        "CREATE TABLE IF NOT EXISTS data_simulations(run_id TEXT PRIMARY KEY)",
        None,
    )?;
    for (id, mode, tables) in &active {
        if *mode != workflow_service::Mode::Simulation {
            continue;
        }
        #[derive(Deserialize)]
        struct Existing {
            run_id: String,
        }
        let exists = sql
            .exec(
                "SELECT run_id FROM data_simulations WHERE run_id=?",
                Some(vec![id.clone().into()]),
            )?
            .to_array::<Existing>()?;
        if exists.iter().any(|row| row.run_id == *id) {
            continue;
        }
        let scope = format!("sim:{id}");
        for table in tables {
            sql.exec("INSERT INTO data_tables(scope,id,body) SELECT ?,id,body FROM data_tables WHERE scope='live' AND id=?",Some(vec![scope.clone().into(),table.clone().into()]))?;
            sql.exec("INSERT INTO data_rows(scope,table_id,key,body) SELECT ?,table_id,key,body FROM data_rows WHERE scope='live' AND table_id=?",Some(vec![scope.clone().into(),table.clone().into()]))?;
        }
        sql.exec(
            "INSERT INTO data_simulations(run_id) VALUES(?)",
            Some(vec![id.clone().into()]),
        )?;
    }
    let ids = serde_json::to_string(&active.iter().map(|(id, _, _)| id).collect::<Vec<_>>())
        .map_err(internal)?;
    sql.exec("DELETE FROM data_rows WHERE scope IN (SELECT 'sim:'||run_id FROM data_simulations WHERE run_id NOT IN (SELECT value FROM json_each(?)))",Some(vec![ids.clone().into()]))?;
    sql.exec("DELETE FROM data_tables WHERE scope IN (SELECT 'sim:'||run_id FROM data_simulations WHERE run_id NOT IN (SELECT value FROM json_each(?)))",Some(vec![ids.clone().into()]))?;
    sql.exec("DELETE FROM data_receipts WHERE run_id IS NOT NULL AND run_id NOT IN (SELECT value FROM json_each(?))",Some(vec![ids.clone().into()]))?;
    sql.exec(
        "DELETE FROM data_simulations WHERE run_id NOT IN (SELECT value FROM json_each(?))",
        Some(vec![ids.into()]),
    )?;
    Ok(())
}
impl WorkflowWorkspace {
    /// Each task's effect, receipt and completion checkpoint share one transaction.
    /// A data/config error rolls back the effect before persisting its failed outcome.
    pub(crate) fn flush_data_tasks(&self, workspace: &mut Workspace) -> Result<()> {
        for _ in 0..100 {
            let Some((id, task, mode, time)) = workspace.pending_data(now()).map_err(internal)?
            else {
                break;
            };
            let mut candidate = workspace.clone();
            let run_id = id.clone();
            let task_id = task.id.clone();
            let failed = Rc::new(RefCell::new(None));
            let failure = failed.clone();
            let result = self.sql_transaction(move |sql| {
                let outcome = if time >= task.deadline_ms {
                    connectors_http::failure(connector_sdk::FailureCode::Timeout, false)
                } else {
                    let prepared = match connectors_data::prepare(&task.node, &task.input) {
                        Ok(prepared) => prepared,
                        Err(code) => {
                            candidate
                                .complete_data(
                                    &run_id,
                                    &task.id,
                                    connectors_http::failure(code, false),
                                    time,
                                )
                                .map_err(internal)?;
                            crate::storage::write_workspace_records(&sql, &mut candidate)?;
                            return Ok(candidate);
                        }
                    };
                    let scope = if mode == workflow_service::Mode::Simulation {
                        format!("sim:{run_id}")
                    } else {
                        "live".into()
                    };
                    let mut store = SqlDataStore::new(
                        sql.clone(),
                        scope,
                        "engine".into(),
                        Some(run_id.clone()),
                        time,
                    )
                    .map_err(internal)?;
                    match connectors_data::apply(&mut store, prepared, &task.idempotency_key, time)
                    {
                        Ok(value) => connector_sdk::Outcome::success(value),
                        Err(error) => {
                            *failure.borrow_mut() = Some(error);
                            return Err(internal("Data task transaction aborted"));
                        }
                    }
                };
                if !candidate
                    .complete_data(&run_id, &task.id, outcome, time)
                    .map_err(internal)?
                {
                    return Err(internal("Data task is no longer pending"));
                }
                crate::storage::write_workspace_records(&sql, &mut candidate)?;
                Ok(candidate)
            });
            match result {
                Ok(updated) => *workspace = updated,
                Err(error) => {
                    let Some(failure) = failed.borrow_mut().take() else {
                        return Err(error);
                    };
                    if failure.code == ErrorCode::Storage {
                        return Err(error);
                    }
                    let mut candidate = workspace.clone();
                    candidate
                        .complete_data(
                            &id,
                            &task_id,
                            connectors_http::failure(
                                connector_sdk::FailureCode::InvalidConfig,
                                false,
                            ),
                            time,
                        )
                        .map_err(internal)?;
                    *workspace = self.sql_transaction(move |sql| {
                        crate::storage::write_workspace_records(&sql, &mut candidate)?;
                        Ok(candidate)
                    })?;
                }
            }
        }
        Ok(())
    }
}
