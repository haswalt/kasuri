use serde_json::{Value, json};
use workflow_service::Workspace;
fn call(ws: &mut Workspace, path: &str, body: Value, now: u64) -> Value {
    ws.handle(
        "POST",
        path,
        &body.to_string().into_bytes(),
        now,
        "trash-test",
    )
    .unwrap()
    .1
}
#[test]
fn trash_disables_new_triggers_retains_history_and_allows_inflight_runs_to_finish() {
    let mut ws = Workspace::default();
    let doc = json!({"schemaVersion":1,"id":"scheduled","name":"Scheduled wait","status":"draft","updatedAt":"","nodes":[{"id":"start","type":"Schedule","label":"Schedule","config":{"cron":"* * * * *"}},{"id":"wait","type":"Wait / Delay","label":"Wait","config":{"delayMs":100000}}],"edges":[{"id":"e","source":"start","target":"wait"}]});
    let (_, doc) = ws
        .handle(
            "PUT",
            "/api/workflows/scheduled",
            doc.to_string().as_bytes(),
            0,
            "",
        )
        .unwrap();
    let doc = call(
        &mut ws,
        "/api/workflows/scheduled/publish",
        json!({"expectedVersion":doc["version"]}),
        0,
    );
    ws.alarm(60000).unwrap();
    assert_eq!(ws.runs.len(), 1);
    assert_eq!(
        ws.runs[0].status().unwrap(),
        workflow_engine::RunStatus::Running
    );
    assert_eq!(
        ws.handle("POST", "/api/workflows/scheduled/archive", b"{}", 65000, "")
            .unwrap_err()
            .status,
        428
    );
    let archived = call(
        &mut ws,
        "/api/workflows/scheduled/archive",
        json!({"expectedVersion":doc["version"]}),
        65000,
    );
    assert_eq!(archived["status"], "paused");
    assert_eq!(archived["lifecycle"]["enabled"], false);
    assert!(archived["archivedAt"].is_string());
    assert_eq!(ws.snapshot().unwrap()["workflows"], json!([]));
    assert_eq!(ws.snapshot().unwrap()["runs"], json!([]));
    assert_eq!(ws.revisions.len(), 1);
    assert_eq!(ws.live_document("scheduled").unwrap_err().status, 404);
    assert_eq!(ws.next_alarm(), Some(160000));
    ws = Workspace::from_json(&ws.checkpoint().unwrap()).unwrap();
    ws.alarm(160000).unwrap();
    assert_eq!(ws.runs.len(), 1);
    assert_eq!(
        ws.runs[0].status().unwrap(),
        workflow_engine::RunStatus::Succeeded
    );
    assert_eq!(ws.next_alarm(), None);
    assert_eq!(
        ws.handle(
            "POST",
            "/api/runs",
            br#"{"workflowId":"scheduled","mode":"run","input":{}}"#,
            160000,
            ""
        )
        .unwrap_err()
        .status,
        404
    );
    assert_eq!(
        ws.handle(
            "POST",
            "/api/workflows/scheduled/publish",
            b"{}",
            160000,
            ""
        )
        .unwrap_err()
        .status,
        409
    );
    let trash = ws
        .handle("GET", "/api/workflows/trash", b"", 160000, "")
        .unwrap()
        .1;
    assert_eq!(trash["workflows"].as_array().unwrap().len(), 1);
    let restored = call(
        &mut ws,
        "/api/workflows/scheduled/restore",
        json!({"expectedVersion":archived["version"]}),
        160001,
    );
    assert_eq!(restored["status"], "paused");
    assert_eq!(restored["archivedAt"], Value::Null);
    assert_eq!(ws.snapshot().unwrap()["runs"].as_array().unwrap().len(), 1);
    assert_eq!(ws.next_alarm(), None);
    assert_eq!(
        ws.snapshot().unwrap()["workflows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let resumed = call(
        &mut ws,
        "/api/workflows/scheduled/resume",
        json!({"expectedVersion":restored["version"]}),
        160002,
    );
    assert_eq!(resumed["status"], "active");
    assert!(
        ws.next_alarm()
            .is_some_and(|at| (180000..181000).contains(&at))
    );
}
#[test]
fn archived_drafts_cannot_be_replaced_and_restore_without_publication() {
    let mut ws = Workspace::default();
    let mut input: Value =
        serde_json::from_str(include_str!("../../../examples/workflows/01-hello.json")).unwrap();
    let (_, doc) = ws
        .handle(
            "PUT",
            "/api/workflows/01-hello",
            input.to_string().as_bytes(),
            0,
            "",
        )
        .unwrap();
    let archived = call(
        &mut ws,
        "/api/workflows/01-hello/archive",
        json!({"expectedVersion":doc["version"]}),
        1,
    );
    input["version"] = archived["version"].clone();
    assert_eq!(
        ws.handle(
            "PUT",
            "/api/workflows/01-hello",
            input.to_string().as_bytes(),
            2,
            ""
        )
        .unwrap_err()
        .status,
        409
    );
    let restored = call(
        &mut ws,
        "/api/workflows/01-hello/restore",
        json!({"expectedVersion":archived["version"]}),
        3,
    );
    assert_eq!(restored["status"], "draft");
    assert_eq!(restored["revision"], 0);
    assert_eq!(restored["lifecycle"]["enabled"], false);
    assert!(ws.revisions.is_empty());
}

#[test]
fn permanent_delete_requires_archived_version_and_removes_workflow_history() {
    let mut ws = Workspace::default();
    let workflow_path = "/api/workflows/01-hello";
    let delete_path = "/api/workflows/01-hello/trash";
    let (_, draft) = ws
        .handle(
            "PUT",
            workflow_path,
            include_bytes!("../../../examples/workflows/01-hello.json"),
            0,
            "",
        )
        .unwrap();
    let published = call(
        &mut ws,
        "/api/workflows/01-hello/publish",
        json!({"expectedVersion": draft["version"]}),
        1,
    );
    let precondition = |doc: &Value| json!({"expectedVersion": doc["version"]}).to_string();
    assert_eq!(
        ws.handle_http_v2(
            "DELETE",
            delete_path,
            precondition(&published).as_bytes(),
            2,
            ""
        )
        .unwrap_err()
        .status,
        409
    );
    let archived = call(
        &mut ws,
        "/api/workflows/01-hello/archive",
        json!({"expectedVersion": published["version"]}),
        3,
    );
    for (input, status) in [(String::from("{}"), 428), (precondition(&published), 409)] {
        assert_eq!(
            ws.handle_http_v2("DELETE", delete_path, input.as_bytes(), 4, "")
                .unwrap_err()
                .status,
            status
        );
    }
    assert_eq!(ws.revisions.len(), 1);
    assert_eq!(
        ws.handle_http_v2(
            "DELETE",
            delete_path,
            precondition(&archived).as_bytes(),
            5,
            "purge"
        )
        .unwrap(),
        (200, json!({"deleted":true}))
    );
    assert!(ws.revisions.is_empty());
    assert_eq!(
        ws.handle("GET", "/api/workflows/trash", b"", 6, "")
            .unwrap()
            .1["workflows"],
        json!([])
    );
    assert_eq!(
        ws.handle("GET", workflow_path, b"", 6, "")
            .unwrap_err()
            .status,
        404
    );
    assert_eq!(ws.audit.last().unwrap()["action"], "workflow.deleted");
}
