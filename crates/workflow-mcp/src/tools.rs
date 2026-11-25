use crate::auth::{Auth, Credential, Receipt, hash};
use serde_json::{Value, json};
use workflow_service::{Document, Workspace};

fn definitions() -> Vec<(&'static str, &'static str, &'static str, bool, Value)> {
    let id = json!({"type":"string","minLength":1,"maxLength":160});
    let list = json!({"type":"object","properties":{"cursor":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":100},"search":{"type":"string","maxLength":160},"workflowId":id,"status":{"type":"string"}},"additionalProperties":false});
    let object = |props: Value, required: Vec<&str>| json!({"type":"object","properties":props,"required":required,"additionalProperties":false});
    let mutate = |extra: Value, required: Vec<&str>| {
        let mut props = extra;
        props["idempotencyKey"] = json!({"type":"string","minLength":8,"maxLength":128});
        let mut req = required;
        req.push("idempotencyKey");
        object(props, req)
    };
    let version = json!({"type":"integer","minimum":0});
    let workflow = json!({"$ref":"#/$defs/WorkflowDefinition"});
    vec![
        (
            "data_tables_list",
            "data_read",
            "List workspace data tables/context namespaces with keyset pagination.",
            false,
            object(
                json!({"cursor":{"type":"string","maxLength":64},"limit":{"type":"integer","minimum":1,"maximum":100}}),
                vec![],
            ),
        ),
        (
            "data_table_get",
            "data_read",
            "Inspect a table/context definition and metadata version.",
            false,
            object(json!({"tableId":id}), vec!["tableId"]),
        ),
        (
            "data_get",
            "data_read",
            "Read one unexpired keyed record. Common credential fields are redacted.",
            false,
            object(
                json!({"tableId":id,"key":{"type":"string","minLength":1,"maxLength":512}}),
                vec!["tableId", "key"],
            ),
        ),
        (
            "data_query",
            "data_read",
            "Query workspace records with bounded keyset cursors and JSON-pointer filters. Empty pages may still have continuation.",
            false,
            object(
                json!({"tableId":id,"query":workflow_data::query_schema()}),
                vec!["tableId"],
            ),
        ),
        (
            "data_apply",
            "data_write",
            "Create/rename a data set or insert/put/patch/delete a record. Patch preserves unmentioned fields and expiry. Mutations use persistent receipts; delete is permanent.",
            true,
            mutate(
                json!({"command":workflow_data::mutation_schema()}),
                vec!["command"],
            ),
        ),
        (
            "data_batch",
            "data_write",
            "Apply up to 100 mutating data commands in one storage transaction. Per-command existing/conflict statuses remain explicit outcomes.",
            true,
            mutate(
                json!({"commands":{"type":"array","minItems":1,"maxItems":100,"items":workflow_data::mutation_schema()}}),
                vec!["commands"],
            ),
        ),
        (
            "workflows_schema_get",
            "read",
            "Get the authoritative versioned graph/config JSON Schema and defaults. Node kind uses type; labels are required. Read before building.",
            false,
            object(json!({}), vec![]),
        ),
        (
            "workflows_examples_list",
            "read",
            "List tested public workflow patterns and dependency IDs; no workspace data or side effects. Fetch a definition with workflows_examples_get.",
            false,
            object(
                json!({"cursor":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":100}}),
                vec![],
            ),
        ),
        (
            "workflows_examples_get",
            "read",
            "Get a tested public workflow, synthetic input, asserted outputs and dependency/publication instructions. Does not install anything.",
            false,
            object(json!({"exampleId":id}), vec!["exampleId"]),
        ),
        (
            "platform_capabilities_get",
            "read",
            "Read workflow execution, input/output wiring, publication, simulation, retry and scope semantics before designing a workflow.",
            false,
            object(json!({}), vec![]),
        ),
        (
            "connectors_get",
            "read",
            "Inspect one implemented connector's configuration and runtime input/output schemas, optionally specialized for supplied config. Includes typed wiring guidance and tested public configurations. Does not read workspace data.",
            false,
            object(
                json!({"kind":{"type":"string"},"version":{"type":"integer","minimum":1},"config":{"type":"object"}}),
                vec!["kind"],
            ),
        ),
        (
            "connectors_list",
            "read",
            "Discover implemented connector versions, config schemas and ports. Planned connectors are excluded.",
            false,
            object(json!({}), vec![]),
        ),
        (
            "workflows_list",
            "read",
            "List/search saved workflow summaries with bounded pagination.",
            false,
            list.clone(),
        ),
        (
            "workflows_get",
            "read",
            "Get a workflow graph and its current optimistic-concurrency version.",
            false,
            object(json!({"workflowId":id}), vec!["workflowId"]),
        ),
        (
            "workflows_revisions",
            "read",
            "Browse retained immutable publications. Legacy baselines are marked; missing historical definitions cannot be reconstructed.",
            false,
            object(
                json!({"workflowId":id,"cursor":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":100}}),
                vec!["workflowId"],
            ),
        ),
        (
            "workflows_restore_revision",
            "write",
            "Restore a retained publication into the draft without changing the live definition or activation. Publishing is a separate operation.",
            true,
            mutate(
                json!({"workflowId":id,"revision":{"type":"integer","minimum":1},"expectedVersion":version}),
                vec!["workflowId", "revision", "expectedVersion"],
            ),
        ),
        (
            "workflows_trash",
            "read",
            "Browse recoverable archived workflows.",
            false,
            list.clone(),
        ),
        (
            "workflows_archive",
            "delete",
            "Move a workflow to recoverable trash and stop new triggers. Existing runs finish; no data is permanently deleted.",
            true,
            mutate(
                json!({"workflowId":id,"expectedVersion":version}),
                vec!["workflowId", "expectedVersion"],
            ),
        ),
        (
            "workflows_restore",
            "delete",
            "Restore an archived workflow without reactivating triggers.",
            true,
            mutate(
                json!({"workflowId":id,"expectedVersion":version}),
                vec!["workflowId", "expectedVersion"],
            ),
        ),
        (
            "workflows_validate",
            "read",
            "Validate schema, topology and connector configuration without saving, publishing or executing.",
            false,
            object(json!({"workflow":workflow}), vec!["workflow"]),
        ),
        (
            "workflows_create",
            "write",
            "Create an unpublished draft. Does not activate triggers or make external requests.",
            true,
            mutate(json!({"workflow":workflow}), vec!["workflow"]),
        ),
        (
            "workflows_update",
            "write",
            "Replace a draft graph/configuration only if expectedVersion still matches. Does not publish.",
            true,
            mutate(
                json!({"workflow":workflow,"expectedVersion":version}),
                vec!["workflow", "expectedVersion"],
            ),
        ),
        (
            "workflows_clone",
            "write",
            "Clone a saved workflow into a new draft with a fresh workflow ID.",
            true,
            mutate(
                json!({"workflowId":id,"name":{"type":"string","maxLength":160}}),
                vec!["workflowId"],
            ),
        ),
        (
            "workflows_publish",
            "publish",
            "Publish the selected version. May activate recurring schedules or webhook triggers.",
            true,
            mutate(
                json!({"workflowId":id,"expectedVersion":version}),
                vec!["workflowId", "expectedVersion"],
            ),
        ),
        (
            "workflows_pause",
            "publish",
            "Pause a workflow's trigger registrations at the selected version.",
            true,
            mutate(
                json!({"workflowId":id,"expectedVersion":version}),
                vec!["workflowId", "expectedVersion"],
            ),
        ),
        (
            "workflows_resume",
            "publish",
            "Resume workflow triggers at the selected version; schedules may execute external actions.",
            true,
            mutate(
                json!({"workflowId":id,"expectedVersion":version}),
                vec!["workflowId", "expectedVersion"],
            ),
        ),
        (
            "runs_simulate",
            "simulate",
            "Run the selected workflow version through the Rust engine with no network calls. HTTP and Clairic nodes require explicit simulationResponse fixtures.",
            true,
            mutate(
                json!({"workflowId":id,"expectedVersion":version,"input":{}}),
                vec!["workflowId", "expectedVersion", "input"],
            ),
        ),
        (
            "runs_start",
            "execute",
            "Start a real run at the selected version. External connectors may make irreversible external writes; client approval is appropriate.",
            true,
            mutate(
                json!({"workflowId":id,"expectedVersion":version,"input":{}}),
                vec!["workflowId", "expectedVersion", "input"],
            ),
        ),
        (
            "runs_list",
            "read",
            "List retained runs with bounded pagination and workflow/status filters.",
            false,
            list,
        ),
        (
            "runs_get",
            "read",
            "Inspect a run's recorded node outcomes, attempts, errors and durable wake times. Historical step inputs and detailed timings are unavailable.",
            false,
            object(json!({"runId":id}), vec!["runId"]),
        ),
        (
            "runs_pause",
            "control",
            "Pause pending dispatch in a run; already dispatched external effects can still complete.",
            true,
            mutate(json!({"runId":id}), vec!["runId"]),
        ),
        (
            "runs_resume",
            "control",
            "Resume a paused run; pending external effects can execute.",
            true,
            mutate(json!({"runId":id}), vec!["runId"]),
        ),
        (
            "runs_cancel",
            "control",
            "Cancel future work in a run. Cannot undo an external write already committed.",
            true,
            mutate(json!({"runId":id}), vec!["runId"]),
        ),
        (
            "audit_list",
            "audit",
            "Read retained append-only management audit records with attribution and retention/gap metadata.",
            false,
            object(
                json!({"after":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":100}}),
                vec![],
            ),
        ),
    ]
}
pub fn list(c: &Credential) -> Vec<Value> {
    definitions().into_iter().filter(|(_,scope,_,_,_)|c.permits("read")&&c.permits(scope)).map(|(name,_,description,mutation,mut schema)|{
        if matches!(name,"workflows_create"|"workflows_update"|"workflows_validate"){
            let mut definition=workflow_service::definition::schema();
            let defs=definition.as_object_mut().unwrap().remove("$defs").unwrap_or(json!({}));
            schema["$defs"]=defs;
            definition.as_object_mut().unwrap().remove("$id");
            definition.as_object_mut().unwrap().remove("$schema");
            schema["$defs"]["WorkflowDefinition"]=definition;
        }
        json!({"name":name,"description":description,"inputSchema":schema,"annotations":{"readOnlyHint":!mutation,"destructiveHint":mutation,"idempotentHint":true,"openWorldHint":name=="runs_start"||name=="runs_resume"||name.starts_with("workflows_p")||name=="workflows_resume"}})
    }).collect()
}

fn err(code: &str, message: &str) -> Value {
    json!({"code":code,"message":message})
}
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a str, Value> {
    v[key]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 160)
        .ok_or_else(|| err("invalid_arguments", &format!("Expected {key}")))
}
fn existing(ws: &Workspace, id: &str) -> Result<Document, Value> {
    ws.workflows
        .iter()
        .find(|w| w.graph.id == id)
        .cloned()
        .ok_or_else(|| err("not_found", "Workflow not found"))
}
fn version(ws: &Workspace, args: &Value) -> Result<String, Value> {
    let id = required(args, "workflowId")?;
    let current = existing(ws, id)?;
    if args["expectedVersion"].as_u64() != Some(current.version) {
        return Err(err(
            "conflict",
            "Workflow changed; fetch workflows_get and retry with its version",
        ));
    };
    Ok(id.into())
}
fn handle(
    ws: &mut Workspace,
    method: &str,
    path: &str,
    body: Value,
    now: u64,
    new_id: &str,
    actor: &str,
) -> Result<Value, Value> {
    ws.handle_as(
        method,
        path,
        &serde_json::to_vec(&body).unwrap(),
        now,
        new_id,
        actor,
    )
    .map(|(_, v)| v)
    .map_err(|e| {
        let mut value = err(
            if e.status == 409 {
                "conflict"
            } else {
                "operation_failed"
            },
            &e.message,
        );
        if !e.diagnostics.is_empty() {
            value["errors"] = json!(e.diagnostics);
        }
        value
    })
}
fn page(items: Vec<Value>, args: &Value) -> Result<Value, Value> {
    let cursor = args["cursor"].as_u64().unwrap_or(0) as usize;
    let limit = args["limit"].as_u64().unwrap_or(25) as usize;
    if !(1..=100).contains(&limit) || cursor > 10000 {
        return Err(err("invalid_arguments", "Invalid page bounds"));
    }
    let next = cursor.saturating_add(limit);
    let total = items.len();
    let items: Vec<_> = items.into_iter().skip(cursor).take(limit).collect();
    Ok(
        json!({"items":items,"nextCursor":if next<total{Some(next)}else{None},"retainedTotal":total}),
    )
}
fn execute(
    ws: &mut Workspace,
    name: &str,
    args: &Value,
    now: u64,
    new_id: &str,
    actor: &str,
    data: Option<&mut dyn crate::DataAccess>,
) -> Result<Value, Value> {
    if name.starts_with("data_") {
        let host = data.ok_or_else(|| {
            err(
                "unsupported",
                "Persistent data access is unavailable in this host",
            )
        })?;
        let convert = |error: workflow_data::DataError| {
            err(
                match error.code {
                    workflow_data::ErrorCode::InvalidRequest => "invalid_arguments",
                    workflow_data::ErrorCode::NotFound => "not_found",
                    workflow_data::ErrorCode::Conflict => "conflict",
                    workflow_data::ErrorCode::Storage => "storage_error",
                },
                &error.message,
            )
        };
        if name == "data_tables_list" {
            if args.get("limit").is_some_and(|v| v.as_u64().is_none())
                || args.get("cursor").is_some_and(|v| !v.is_string())
            {
                return Err(err(
                    "invalid_arguments",
                    "Expected an integer limit and string cursor",
                ));
            }
            let limit = args["limit"].as_u64().unwrap_or(25);
            if !(1..=100).contains(&limit) {
                return Err(err("invalid_arguments", "Use a limit between 1 and 100"));
            }
            let limit = limit as usize;
            return host
                .tables(args["cursor"].as_str(), limit, now)
                .map_err(convert);
        }
        let (commands,batch,receipt)=match name {
            "data_table_get"=>(vec![workflow_data::Command::GetTable{table_id:required(args,"tableId")?.into()}],false,None),
            "data_get"=>(vec![serde_json::from_value::<workflow_data::Command>(json!({"operation":"get","tableId":args["tableId"],"key":args["key"]})).map_err(|_|err("invalid_arguments","Invalid keyed read"))?],false,None),
            "data_query"=>(vec![serde_json::from_value::<workflow_data::Command>(json!({"operation":"query","tableId":args["tableId"],"query":args.get("query").cloned().unwrap_or(json!({}))})).map_err(|_|err("invalid_arguments","Invalid query"))?],false,None),
            "data_apply"=>(vec![serde_json::from_value::<workflow_data::Command>(args["command"].clone()).map_err(|_|err("invalid_arguments","Invalid data command"))?],false,args["idempotencyKey"].as_str()),
            "data_batch"=>(serde_json::from_value::<Vec<workflow_data::Command>>(args["commands"].clone()).map_err(|_|err("invalid_arguments","Invalid data batch"))?,true,args["idempotencyKey"].as_str()),
            _=>return Err(err("unknown_tool","Data tool not found")),
        };
        if receipt.is_some()
            && (commands.iter().any(|command| !command.mutates())
                || serde_json::to_string(&commands)
                    .unwrap_or_default()
                    .contains("[REDACTED]"))
        {
            return Err(err(
                "invalid_arguments",
                "Use read tools for reads; replace redaction placeholders explicitly or patch only known fields",
            ));
        }
        return host
            .execute(ws, commands, batch, receipt, actor, now)
            .map_err(convert);
    }
    match name {
        "connectors_list" => Ok(crate::discovery::connectors()),
        "connectors_get" => {
            let kind = required(args, "kind")?;
            let version = args
                .get("version")
                .map(|v| {
                    v.as_u64()
                        .filter(|n| *n >= 1)
                        .and_then(|n| u32::try_from(n).ok())
                })
                .unwrap_or(Some(1))
                .ok_or_else(|| err("invalid_arguments", "Use a positive connector version"))?;
            let empty = serde_json::Map::new();
            let config = match args.get("config") {
                Some(value) => value
                    .as_object()
                    .ok_or_else(|| err("invalid_arguments", "Expected config object"))?,
                None => &empty,
            };
            crate::discovery::connector(kind, version, config).ok_or_else(|| {
                err(
                    "unsupported_connector",
                    "Connector kind/version is not implemented",
                )
            })
        }
        "workflows_schema_get" => Ok(workflow_service::definition::discovery()),
        "platform_capabilities_get" => Ok(crate::discovery::capabilities()),
        "workflows_examples_list" => page(
            crate::discovery::examples()
                .into_iter()
                .map(|mut e| {
                    e.as_object_mut().unwrap().remove("workflow");
                    e.as_object_mut().unwrap().remove("expectedOutputs");
                    e
                })
                .collect(),
            args,
        ),
        "workflows_examples_get" => {
            let id = required(args, "exampleId")?;
            crate::discovery::examples()
                .into_iter()
                .find(|e| e["id"] == id)
                .ok_or_else(|| err("not_found", "Public example not found"))
        }
        "workflows_list" => {
            let query = args["search"].as_str().unwrap_or("").to_lowercase();
            page(ws.workflows.iter().filter(|w|w.archived_at.is_none() && w.graph.name.to_lowercase().contains(&query)).map(|w|json!({"id":w.graph.id,"name":w.graph.name,"revision":w.graph.revision,"version":w.version,"status":w.status,"lifecycle":w.lifecycle,"hasUnpublishedChanges":w.has_unpublished_changes,"updatedAt":w.updated_at,"nodeCount":w.graph.nodes.len(),"nextScheduledAt":w.next_scheduled_at})).collect(),args)
        }
        "workflows_trash" => page(
            ws.workflows
                .iter()
                .filter(|w| w.archived_at.is_some())
                .map(|w| serde_json::to_value(w).unwrap())
                .collect(),
            args,
        ),
        "workflows_archive" | "workflows_restore" => {
            let id = version(ws, args)?;
            let action = if name == "workflows_archive" {
                "archive"
            } else {
                "restore"
            };
            handle(
                ws,
                "POST",
                &format!("/api/workflows/{id}/{action}"),
                json!({"expectedVersion":args["expectedVersion"]}),
                now,
                new_id,
                actor,
            )
        }
        "workflows_get" => {
            Ok(serde_json::to_value(existing(ws, required(args, "workflowId")?)?).unwrap())
        }
        "workflows_revisions" => {
            let id = required(args, "workflowId")?;
            existing(ws, id)?;
            page(
                ws.revisions
                    .iter()
                    .rev()
                    .filter(|r| r.workflow_id == id)
                    .map(|r| serde_json::to_value(r).unwrap())
                    .collect(),
                args,
            )
        }
        "workflows_restore_revision" => {
            let id = version(ws, args)?;
            let revision = args["revision"]
                .as_u64()
                .ok_or_else(|| err("invalid_arguments", "Expected revision"))?;
            handle(
                ws,
                "POST",
                &format!("/api/workflows/{id}/revisions/{revision}/restore"),
                json!({"expectedVersion":args["expectedVersion"]}),
                now,
                new_id,
                actor,
            )
        }
        "workflows_validate" => {
            if !args["workflow"].is_object() {
                return Err(err("invalid_arguments", "Expected workflow object"));
            };
            Ok(workflow_service::definition::validate(&args["workflow"]))
        }
        "workflows_create" | "workflows_update" => {
            let mut doc = args["workflow"].clone();
            if !doc.is_object() {
                return Err(err("invalid_arguments", "Expected workflow object"));
            };
            let errors = workflow_service::definition::shape_errors(&doc);
            if !errors.is_empty() {
                return Err(
                    json!({"code":"invalid_workflow","message":"Invalid workflow definition","errors":errors}),
                );
            }
            let id = required(&doc, "id")?.to_string();
            if name == "workflows_create" {
                if ws.workflows.iter().any(|w| w.graph.id == id) {
                    return Err(err("conflict", "Workflow already exists"));
                }
                doc["version"] = json!(0)
            } else {
                let current = existing(ws, &id)?;
                if doc.to_string().contains("[REDACTED]") {
                    return Err(err(
                        "redacted_configuration",
                        "Cannot save redaction placeholders. Use named secret aliases or replace the redacted fields explicitly.",
                    ));
                };
                if args["expectedVersion"].as_u64() != Some(current.version) {
                    return Err(err("conflict", "Workflow changed; reload before saving"));
                }
                doc["version"] = json!(current.version)
            }
            doc["status"] = json!("draft");
            doc["updatedAt"] = json!("");
            handle(
                ws,
                "PUT",
                &format!("/api/workflows/{id}"),
                doc,
                now,
                new_id,
                actor,
            )
        }
        "workflows_clone" => {
            let source = existing(ws, required(args, "workflowId")?)?;
            let mut doc = serde_json::to_value(source).unwrap();
            doc["id"] = json!(new_id);
            doc["version"] = json!(0);
            doc["name"] = json!(args["name"].as_str().unwrap_or("Cloned workflow"));
            handle(
                ws,
                "PUT",
                &format!("/api/workflows/{new_id}"),
                doc,
                now,
                new_id,
                actor,
            )
        }
        "workflows_publish" | "workflows_pause" | "workflows_resume" => {
            let id = version(ws, args)?;
            let action = name.strip_prefix("workflows_").unwrap();
            handle(
                ws,
                "POST",
                &format!("/api/workflows/{id}/{action}"),
                json!({"expectedVersion":args["expectedVersion"]}),
                now,
                new_id,
                actor,
            )
        }
        "runs_simulate" | "runs_start" => {
            let id = version(ws, args)?;
            handle(
                ws,
                "POST",
                "/api/runs",
                json!({"workflowId":id,"expectedVersion":args["expectedVersion"],"mode":if name=="runs_simulate"{"simulation"}else{"run"},"input":args["input"]}),
                now,
                new_id,
                actor,
            )
        }
        "runs_list" => {
            // Every page `page` can return: the furthest cursor plus the largest limit.
            let items = ws
                .list_runs(10_100, args["workflowId"].as_str(), args["status"].as_str())
                .map_err(|_| err("internal", "Cannot read runs"))?;
            page(items, args)
        }
        "runs_get" => {
            let id = required(args, "runId")?;
            handle(
                ws,
                "GET",
                &format!("/api/runs/{id}"),
                Value::Null,
                now,
                new_id,
                actor,
            )
        }
        "runs_pause" | "runs_resume" | "runs_cancel" => {
            let id = required(args, "runId")?;
            let action = name.strip_prefix("runs_").unwrap();
            handle(
                ws,
                "POST",
                &format!("/api/runs/{id}/{action}"),
                json!({}),
                now,
                new_id,
                actor,
            )
        }
        "audit_list" => {
            let limit = args["limit"].as_u64().unwrap_or(25);
            if !(1..=100).contains(&limit) {
                return Err(err("invalid_arguments", "Invalid page limit"));
            }
            Ok(ws.audit_page(args["after"].as_u64().unwrap_or(0), limit as usize))
        }
        _ => Err(err("unknown_tool", "Tool not found")),
    }
}
/// Defense in depth for inline credentials. Host-managed secrets are never read here.
pub fn redact(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let key = k.to_lowercase().replace(['-', '_'], "");
                    let sensitive = [
                        "authorization",
                        "password",
                        "secret",
                        "token",
                        "cookie",
                        "apikey",
                    ]
                    .iter()
                    .any(|s| key.contains(s));
                    let alias =
                        matches!(k.as_str(), "$secret" | "passwordSecret" | "signingSecret")
                            || v.as_object().is_some_and(|o| {
                                o.len() == 1 && o.get("$secret").is_some_and(Value::is_string)
                            });
                    (
                        k,
                        if sensitive && !alias {
                            json!("[REDACTED]")
                        } else {
                            redact(v)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(redact).collect()),
        Value::String(ref s) if s.trim_start().starts_with(['{', '[']) => {
            serde_json::from_str::<Value>(s)
                .map(|v| Value::String(redact(v).to_string()))
                .unwrap_or(value)
        }
        other => other,
    }
}
fn compact_data(value: Value) -> Value {
    if let Some(results) = value["results"].as_array() {
        return json!({"results":results.iter().cloned().map(compact_data).collect::<Vec<_>>()});
    }
    json!({"status":value["status"],"imported":value["imported"],"skippedExpired":value["skippedExpired"],"inserted":value["inserted"],"deleted":value["deleted"],"rowVersion":value["row"]["version"],"expiresAt":value["row"]["expiresAt"],"tableId":value["table"]["definition"]["id"],"version":value["table"]["version"],"dataRevision":value.get("dataRevision").or_else(||value.pointer("/table/dataRevision")),"detailsTool":if value.get("imported").is_some(){"data_query"}else if value["table"].is_object(){"data_table_get"}else{"data_get"}})
}
fn content(result: Result<Value, Value>) -> Value {
    let error = result.is_err();
    let data = redact(result.unwrap_or_else(|e| e));
    json!({"isError":error,"content":[{"type":"text","text":data.to_string()}],"structuredContent":data})
}
pub fn call(
    ws: &mut Workspace,
    auth: &mut Auth,
    c: &Credential,
    params: &Value,
    now: u64,
    new_id: &str,
) -> Value {
    call_with_data(ws, auth, c, params, now, new_id, None)
}
pub fn call_with_data(
    ws: &mut Workspace,
    auth: &mut Auth,
    c: &Credential,
    params: &Value,
    now: u64,
    new_id: &str,
    data: Option<&mut dyn crate::DataAccess>,
) -> Value {
    let name = params["name"].as_str().unwrap_or("");
    let args = &params["arguments"];
    let Some((_, scope, _, mutation, schema)) =
        definitions().into_iter().find(|(n, _, _, _, _)| *n == name)
    else {
        return content(Err(err("unknown_tool", "Tool not found")));
    };
    if !c.permits("read") || !c.permits(scope) {
        if mutation && !name.starts_with("runs_") {
            ws.record_audit(
                &c.id,
                "mcp.permission_denied",
                args["workflowId"].as_str().unwrap_or(""),
                "forbidden",
                now,
                new_id,
            );
            if let Some(event) = ws.audit.last_mut() {
                event["attemptedAction"] = json!(name);
                event["requiredScope"] = json!(scope);
            }
        }
        return content(Err(err("forbidden", "Credential lacks the required scope")));
    }
    if !args.is_object()
        || schema["required"]
            .as_array()
            .is_some_and(|keys| keys.iter().any(|k| args.get(k.as_str().unwrap()).is_none()))
        || args
            .as_object()
            .is_some_and(|a| a.keys().any(|k| schema["properties"].get(k).is_none()))
    {
        return content(Err(err(
            "invalid_arguments",
            "Arguments do not match the tool schema",
        )));
    }
    if matches!(
        name,
        "runs_start"
            | "runs_simulate"
            | "runs_resume"
            | "runs_get"
            | "workflows_publish"
            | "workflows_resume"
    ) {
        let (graph, simulation) =
            if matches!(name, "runs_start" | "runs_simulate" | "workflows_publish") {
                (
                    ws.workflows
                        .iter()
                        .find(|w| Some(w.graph.id.as_str()) == args["workflowId"].as_str())
                        .map(|w| w.graph.clone()),
                    name == "runs_simulate",
                )
            } else if name == "workflows_resume" {
                (
                    args["workflowId"]
                        .as_str()
                        .and_then(|id| ws.live_document(id).ok())
                        .map(|w| w.graph),
                    false,
                )
            } else {
                args["runId"]
                    .as_str()
                    .and_then(|id| ws.find_run(id).ok().flatten())
                    .and_then(|r| {
                        let checkpoint = workflow_service::checkpoint_state(&r.checkpoint).ok()?;
                        Some((
                            serde_json::from_value::<workflow_schema::Workflow>(
                                checkpoint["workflow"].clone(),
                            )
                            .ok(),
                            r.mode == workflow_service::Mode::Simulation,
                        ))
                    })
                    .unwrap_or((None, false))
            };
        if let Some(graph) = graph {
            let targets = if matches!(name, "runs_get" | "runs_resume") {
                args["runId"]
                    .as_str()
                    .and_then(|id| ws.find_run(id).ok().flatten())
                    .map(|r| r.invocation_targets.clone())
                    .unwrap_or_default()
            } else {
                match ws.invocation_targets(&graph) {
                    Ok(targets) => targets,
                    Err(_) => {
                        return content(Err(err(
                            "invalid_arguments",
                            "Invoked workflow is unavailable or invalid",
                        )));
                    }
                }
            };
            for node in graph
                .nodes
                .iter()
                .chain(targets.values().flat_map(|graph| graph.nodes.iter()))
            {
                if !connectors_data::handles(&node.kind) {
                    continue;
                }
                let Ok(access) = connectors_data::access(node) else {
                    return content(Err(err("invalid_arguments", "Invalid data task")));
                };
                if !c.permits("data_read")
                    || (name != "runs_get"
                        && !simulation
                        && access.writes
                        && !c.permits("data_write"))
                {
                    return content(Err(err(
                        "forbidden",
                        "Data workflows require explicit data_read and live mutations require data_write",
                    )));
                }
            }
        }
    }
    let fingerprint = hash(&json!({"name":name,"arguments":args}).to_string());
    let key = args["idempotencyKey"].as_str().unwrap_or("");
    if mutation {
        auth.prune(now);
        if !(8..=128).contains(&key.len()) {
            return content(Err(err(
                "invalid_arguments",
                "Mutations require an 8–128 character idempotencyKey",
            )));
        }
        if let Some(receipt) = auth
            .receipts
            .iter()
            .find(|r| r.actor == c.id && r.key == key)
        {
            return if receipt.fingerprint == fingerprint {
                receipt.result.clone()
            } else {
                content(Err(err(
                    "conflict",
                    "Idempotency key was used for different arguments",
                )))
            };
        }
        if auth.receipts.len() >= 64 {
            return content(Err(err(
                "capacity",
                "Idempotency receipt window is full; retry after older receipts expire",
            )));
        }
    }
    if mutation
        && name.starts_with("data_")
        && serde_json::to_string(auth)
            .map(|s| s.len() + 66_560 > 524_288)
            .unwrap_or(true)
    {
        return content(Err(err(
            "capacity",
            "Receipt storage is full; inspect existing state before retrying later",
        )));
    }
    let result=execute(ws,name,args,now,new_id,&c.id,data).map(|v| {
        if mutation && name.starts_with("data_") {return compact_data(v);}

        if mutation { json!({"id":v["id"],"workflowId":v["workflowId"],"version":v["version"],"revision":v["revision"],"status":v["status"],"detailsTool":if name.starts_with("runs_"){"runs_get"}else{"workflows_get"}}) } else { v }
    });
    // These payloads are compiled public contracts/fixtures and never read workspace
    // data. connectors_get also echoes the structure of the caller's own request
    // (mapping keys, pointers, declared schemas) back to that caller; literal mapping
    // values are reduced to types and never returned. Key-based credential redaction
    // would corrupt schemas (e.g. password properties), so it is not applied here.
    let public = matches!(
        name,
        "workflows_schema_get"
            | "connectors_get"
            | "platform_capabilities_get"
            | "workflows_examples_list"
            | "workflows_examples_get"
            | "connectors_list"
    );
    let mut result = match result {
        Ok(data) if public => {
            json!({"isError":false,"content":[{"type":"text","text":data.to_string()}],"structuredContent":data})
        }
        other => content(other),
    };
    if result.to_string().len() > 131072 {
        result = content(Err(err(
            "response_too_large",
            "Result exceeds the MCP response budget; narrow the request",
        )))
    }
    if mutation {
        auth.receipts.push(Receipt {
            actor: c.id.clone(),
            key: key.into(),
            fingerprint,
            result: result.clone(),
            expires: now + 86400000,
        })
    }
    result
}
