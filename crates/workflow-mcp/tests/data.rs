use serde_json::{Value, json};
use workflow_data::{Command, MemoryStore};
use workflow_mcp::{DataAccess, auth::Auth, role, rpc_with_data};
use workflow_service::Workspace;
const RESOURCE: &str = "https://kasuri.example/mcp/data-test";
#[derive(Default)]
struct Host {
    store: MemoryStore,
    calls: usize,
}
impl DataAccess for Host {
    fn tables(&mut self, _: Option<&str>, _: usize, _: u64) -> workflow_data::Result<Value> {
        Ok(json!({"tables":self.store.tables.values().collect::<Vec<_>>()}))
    }
    fn execute(
        &mut self,
        workspace: &mut Workspace,
        commands: Vec<Command>,
        batch: bool,
        receipt: Option<&str>,
        actor: &str,
        now: u64,
    ) -> workflow_data::Result<Value> {
        self.calls += 1;
        let key = receipt.map(|key| format!("{actor}:{key}"));
        let before = self.store.tables.clone();
        let result = if batch {
            self.store
                .execute_batch(commands, key.as_deref().unwrap(), now)
        } else {
            self.store
                .execute(commands.into_iter().next().unwrap(), key.as_deref(), now)
        }?;
        if before != self.store.tables {
            workspace.record_audit(actor, "data.changed", "data-test", "succeeded", now, "");
        }
        Ok(result)
    }
}
fn call(
    ws: &mut Workspace,
    auth: &mut Auth,
    host: &mut Host,
    token: &str,
    name: &str,
    args: Value,
) -> Value {
    let credential = auth.authenticate(token, RESOURCE, 100).unwrap();
    rpc_with_data(ws,auth,&credential,json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}),100,"nonce",Some(host)).unwrap()["result"].clone()
}
#[test]
fn explicit_data_scopes_and_durable_receipts_preserve_existing_grants() {
    let mut auth = Auth::default();
    let mut ws = Workspace::default();
    let mut host = Host::default();
    let mut entropy_value = 0;
    let mut entropy = || {
        entropy_value += 1;
        format!("{entropy_value:064x}")
    };
    let reader = auth
        .issue(
            "Existing reader",
            role("reader"),
            RESOURCE,
            0,
            86400000,
            &mut entropy,
        )
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let viewer = auth
        .issue(
            "Data viewer",
            role("data_viewer"),
            RESOURCE,
            0,
            86400000,
            &mut entropy,
        )
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let editor = auth
        .issue(
            "Data editor",
            role("data_editor"),
            RESOURCE,
            0,
            86400000,
            &mut entropy,
        )
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        call(
            &mut ws,
            &mut auth,
            &mut host,
            &reader,
            "data_tables_list",
            json!({})
        )["isError"]
            .as_bool()
            .unwrap()
    );
    let create = json!({"command":{"operation":"create_table","definition":{"id":"context","name":"Context","kind":"context"}},"idempotencyKey":"create-context"});
    assert_eq!(
        call(
            &mut ws,
            &mut auth,
            &mut host,
            &viewer,
            "data_apply",
            create.clone()
        )["structuredContent"]["code"],
        "forbidden"
    );
    let created = call(
        &mut ws,
        &mut auth,
        &mut host,
        &editor,
        "data_apply",
        create.clone(),
    );
    assert_eq!(created["isError"], false);
    let count = host.calls;
    assert_eq!(
        call(&mut ws, &mut auth, &mut host, &editor, "data_apply", create),
        created
    );
    assert_eq!(host.calls, count);
    let put = json!({"command":{"operation":"put","tableId":"context","key":"state","value":{"count":1,"token":"synthetic-private"}},"idempotencyKey":"write-context"});
    let before_auth = serde_json::to_value(&auth).unwrap();
    let stored = call(
        &mut ws,
        &mut auth,
        &mut host,
        &editor,
        "data_apply",
        put.clone(),
    );
    assert!(!stored.to_string().contains("synthetic-private"));
    let version = stored["structuredContent"]["rowVersion"].clone();
    // Simulate lost transport receipt after the host effect/audit was committed.
    auth = serde_json::from_value(before_auth).unwrap();
    let audit_count = ws.audit.len();
    assert_eq!(
        call(&mut ws, &mut auth, &mut host, &editor, "data_apply", put),
        stored
    );
    assert_eq!(ws.audit.len(), audit_count);
    assert_eq!(host.store.tables["context"].data_revision, 1);
    let read = call(
        &mut ws,
        &mut auth,
        &mut host,
        &viewer,
        "data_get",
        json!({"tableId":"context","key":"state"}),
    );
    assert_eq!(
        read["structuredContent"]["row"]["value"]["token"],
        "[REDACTED]"
    );
    let rejected = call(
        &mut ws,
        &mut auth,
        &mut host,
        &editor,
        "data_apply",
        json!({"command":{"operation":"put","tableId":"context","key":"state","value":read["structuredContent"]["row"]["value"]},"idempotencyKey":"reject-redacted"}),
    );
    assert_eq!(rejected["isError"], true);
    let patched = call(
        &mut ws,
        &mut auth,
        &mut host,
        &editor,
        "data_apply",
        json!({"command":{"operation":"patch","tableId":"context","key":"state","patch":{"count":2},"expectedVersion":version},"idempotencyKey":"patch-known-field"}),
    );
    assert_eq!(patched["isError"], false);
    assert_eq!(
        host.store.rows[&("context".into(), "state".into())].value["token"],
        "synthetic-private"
    );
    assert_eq!(
        call(
            &mut ws,
            &mut auth,
            &mut host,
            &editor,
            "workflows_create",
            json!({"workflow":{},"idempotencyKey":"not-workflow-builder"})
        )["structuredContent"]["code"],
        "forbidden"
    );
}
#[test]
fn receipt_capacity_is_checked_before_any_persistent_data_effect() {
    let mut auth = Auth::default();
    let mut ws = Workspace::default();
    let mut host = Host::default();
    let mut n = 0;
    let mut entropy = || {
        n += 1;
        format!("{n:064x}")
    };
    let token = auth
        .issue(
            "Editor",
            role("data_editor"),
            RESOURCE,
            0,
            86400000,
            &mut entropy,
        )
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    auth.receipts.push(workflow_mcp::auth::Receipt {
        actor: "padding".into(),
        key: "padding".into(),
        fingerprint: "padding".into(),
        result: json!({"payload":"x".repeat(480_000)}),
        expires: 86400000,
    });
    let result = call(
        &mut ws,
        &mut auth,
        &mut host,
        &token,
        "data_apply",
        json!({"command":{"operation":"create_table","definition":{"id":"capacity","name":"Capacity","kind":"context"}},"idempotencyKey":"capacity-probe"}),
    );
    assert_eq!(result["structuredContent"]["code"], "capacity");
    assert_eq!(host.calls, 0);
    assert!(host.store.tables.is_empty());
}

#[test]
fn data_workflow_execution_and_run_outputs_require_opt_in_scopes() {
    let mut ws = Workspace::default();
    let mut auth = Auth::default();
    let mut host = Host::default();
    let graph = json!({"schemaVersion":1,"id":"data-flow","name":"Data flow","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Manual Trigger","label":"Start","config":{}},{"id":"data","type":"Context Store","label":"Data","config":{"command":{"operation":"put","tableId":"cache","key":"x","value":1}}}],"edges":[{"id":"e","source":"start","target":"data"}]});
    ws.handle(
        "PUT",
        "/api/workflows/data-flow",
        &serde_json::to_vec(&graph).unwrap(),
        0,
        "",
    )
    .unwrap();
    let mut counter = 0;
    let mut entropy = || {
        counter += 1;
        format!("{counter:064x}")
    };
    let mut issue = |scopes: Vec<String>| {
        auth.issue("Scope test", scopes, RESOURCE, 0, 86400000, &mut entropy)
            .unwrap()["token"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let operator = issue(role("operator"));
    let mut read_scopes = role("operator");
    read_scopes.push("data_read".into());
    let viewer = issue(read_scopes);
    let manager = issue(role("manager"));
    let args = json!({"workflowId":"data-flow","expectedVersion":1,"input":{},"idempotencyKey":"simulation-key"});
    assert_eq!(
        call(
            &mut ws,
            &mut auth,
            &mut host,
            &operator,
            "runs_simulate",
            args.clone()
        )["structuredContent"]["code"],
        "forbidden"
    );
    assert_eq!(
        call(
            &mut ws,
            &mut auth,
            &mut host,
            &viewer,
            "runs_start",
            args.clone()
        )["structuredContent"]["code"],
        "forbidden"
    );
    assert_eq!(
        call(
            &mut ws,
            &mut auth,
            &mut host,
            &operator,
            "workflows_publish",
            json!({"workflowId":"data-flow","expectedVersion":1,"idempotencyKey":"publish-key"})
        )["structuredContent"]["code"],
        "forbidden"
    );
    let result = call(
        &mut ws,
        &mut auth,
        &mut host,
        &viewer,
        "runs_simulate",
        args.clone(),
    );
    assert_eq!(result["isError"], false, "{result}");
    let id = result["structuredContent"]["id"].as_str().unwrap();
    assert_eq!(
        call(
            &mut ws,
            &mut auth,
            &mut host,
            &operator,
            "runs_get",
            json!({"runId":id})
        )["structuredContent"]["code"],
        "forbidden"
    );
    assert_eq!(
        call(
            &mut ws,
            &mut auth,
            &mut host,
            &viewer,
            "runs_get",
            json!({"runId":id})
        )["isError"],
        false
    );
    assert_eq!(
        call(&mut ws, &mut auth, &mut host, &manager, "runs_start", args)["isError"],
        false
    );
}
