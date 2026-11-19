use crate::auth::{Auth, Credential, Receipt, hash};
use serde_json::{Value, json};
use workflow_service::{Document, Workspace, manifests};

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
    let workflow = json!({"type":"object","description":"A schemaVersion 1 Kasuri workflow export, with stable node/edge IDs, connector versions, configuration and optional canvas positions.","required":["schemaVersion","id","name","nodes","edges"]});
    vec![
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
            "Run the selected workflow version through the Rust engine with no network calls. HTTP nodes require explicit simulationResponse fixtures.",
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
    definitions().into_iter().filter(|(_,scope,_,_,_)|c.permits("read")&&c.permits(scope)).map(|(name,_,description,mutation,schema)|json!({"name":name,"description":description,"inputSchema":schema,"annotations":{"readOnlyHint":!mutation,"destructiveHint":mutation,"idempotentHint":true,"openWorldHint":name=="runs_start"||name=="runs_resume"||name.starts_with("workflows_p")||name=="workflows_resume"}})).collect()
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
    nonce: &str,
    actor: &str,
) -> Result<Value, Value> {
    ws.handle_as(
        method,
        path,
        &serde_json::to_vec(&body).unwrap(),
        now,
        nonce,
        actor,
    )
    .map(|(_, v)| v)
    .map_err(|e| {
        err(
            if e.status == 409 {
                "conflict"
            } else {
                "operation_failed"
            },
            &e.message,
        )
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
    nonce: &str,
    actor: &str,
) -> Result<Value, Value> {
    match name {
        "connectors_list" => Ok(json!({"connectors":manifests()})),
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
                nonce,
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
                nonce,
                actor,
            )
        }
        "workflows_validate" => {
            if !args["workflow"].is_object() {
                return Err(err("invalid_arguments", "Expected workflow object"));
            };
            let mut doc = args["workflow"].clone();
            doc["status"] = json!("draft");
            doc["updatedAt"] = json!("");
            let result = serde_json::from_value::<Document>(doc)
                .map_err(|_| "Invalid workflow document".to_string())
                .and_then(|d| Workspace::executable(&d).map_err(|e| e.message));
            Ok(match result {
                Ok(()) => json!({"valid":true,"errors":[]}),
                Err(message) => json!({"valid":false,"errors":[{"message":message}]}),
            })
        }
        "workflows_create" | "workflows_update" => {
            let mut doc = args["workflow"].clone();
            if !doc.is_object() {
                return Err(err("invalid_arguments", "Expected workflow object"));
            };
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
                nonce,
                actor,
            )
        }
        "workflows_clone" => {
            let source = existing(ws, required(args, "workflowId")?)?;
            let mut doc = serde_json::to_value(source).unwrap();
            doc["id"] = json!(nonce);
            doc["version"] = json!(0);
            doc["name"] = json!(args["name"].as_str().unwrap_or("Cloned workflow"));
            handle(
                ws,
                "PUT",
                &format!("/api/workflows/{nonce}"),
                doc,
                now,
                nonce,
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
                nonce,
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
                nonce,
                actor,
            )
        }
        "runs_list" => {
            let mut items = vec![];
            for r in &ws.runs {
                let v = r
                    .view(true)
                    .map_err(|_| err("internal", "Cannot read run"))?;
                if args["workflowId"]
                    .as_str()
                    .is_some_and(|s| v["workflowId"] != s)
                    || args["status"].as_str().is_some_and(|s| v["status"] != s)
                {
                    continue;
                }
                items.push(v)
            }
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
                nonce,
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
                nonce,
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
    nonce: &str,
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
                nonce,
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
    let result=execute(ws,name,args,now,nonce,&c.id).map(|v| {
        if mutation { json!({"id":v["id"],"workflowId":v["workflowId"],"version":v["version"],"revision":v["revision"],"status":v["status"],"detailsTool":if name.starts_with("runs_"){"runs_get"}else{"workflows_get"}}) } else { v }
    });
    let mut result = content(result);
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
