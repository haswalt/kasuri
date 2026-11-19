use serde_json::{Value, json};
use workflow_service::Workspace;
#[test]
fn http_v2_rejects_missing_malformed_and_stale_preconditions_without_mutating_graph() {
    let mut ws = Workspace::default();
    let fixture: Value =
        serde_json::from_str(include_str!("../../../examples/workflows/01-hello.json")).unwrap();
    let (_, mut doc) = ws
        .handle_http_v2(
            "PUT",
            "/api/workflows/01-hello",
            fixture.to_string().as_bytes(),
            0,
            "",
        )
        .unwrap();
    doc["name"] = json!("Updated by HTTP v2");
    doc.as_object_mut().unwrap().remove("version");
    assert_eq!(
        ws.handle_http_v2(
            "PUT",
            "/api/workflows/01-hello",
            doc.to_string().as_bytes(),
            1,
            ""
        )
        .unwrap_err()
        .status,
        428
    );
    assert_ne!(ws.workflows[0].graph.name, "Updated by HTTP v2");
    doc["version"] = json!("1");
    assert_eq!(
        ws.handle_http_v2(
            "PUT",
            "/api/workflows/01-hello",
            doc.to_string().as_bytes(),
            1,
            ""
        )
        .unwrap_err()
        .status,
        400
    );
    doc["version"] = json!(0);
    assert_eq!(
        ws.handle_http_v2(
            "PUT",
            "/api/workflows/01-hello",
            doc.to_string().as_bytes(),
            1,
            ""
        )
        .unwrap_err()
        .status,
        409
    );
    doc["version"] = json!(1);
    let (_, doc) = ws
        .handle_http_v2(
            "PUT",
            "/api/workflows/01-hello",
            doc.to_string().as_bytes(),
            2,
            "",
        )
        .unwrap();
    assert_eq!(doc["version"], 2);
    for action in ["publish", "pause", "resume", "archive", "restore"] {
        assert_eq!(
            ws.handle_http_v2(
                "POST",
                &format!("/api/workflows/01-hello/{action}"),
                b"{}",
                3,
                ""
            )
            .unwrap_err()
            .status,
            428
        );
    }
    let audit_count = ws.audit.len();
    assert_eq!(
        ws.handle_http_v2(
            "POST",
            "/api/runs",
            br#"{"workflowId":"01-hello","mode":"simulation","input":{}}"#,
            3,
            ""
        )
        .unwrap_err()
        .status,
        428
    );
    assert_eq!(
        ws.audit.len(),
        audit_count,
        "Execution rejection is not a management audit event"
    );
    assert!(ws.runs.is_empty());
    ws.handle_http_v2(
        "POST",
        "/api/workflows/01-hello/publish",
        br#"{"expectedVersion":2}"#,
        4,
        "",
    )
    .unwrap();
    assert_eq!(ws.workflows[0].graph.revision, 1);
    assert_eq!(ws.snapshot().unwrap()["apiVersion"], 2);
}
