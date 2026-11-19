use serde_json::{Value, json};
use workflow_service::Workspace;
fn save(ws: &mut Workspace, value: &Value, now: u64) -> Value {
    ws.handle(
        "PUT",
        "/api/workflows/01-hello",
        value.to_string().as_bytes(),
        now,
        "audit-check",
    )
    .unwrap()
    .1
}
#[test]
fn management_events_explain_changes_without_configuration_values_or_read_run_noise() {
    let mut ws = Workspace::default();
    let fixture: Value =
        serde_json::from_str(include_str!("../../../examples/workflows/01-hello.json")).unwrap();
    let mut doc = save(&mut ws, &fixture, 0);
    assert_eq!(ws.audit.len(), 1);
    assert_eq!(ws.audit[0]["action"], "workflow.created");
    assert_eq!(ws.audit[0]["entity"]["name"], doc["name"]);
    assert_eq!(save(&mut ws, &doc, 1), doc);
    assert_eq!(ws.audit.len(), 1);
    doc["nodes"][1]["config"]["mapping"]["message"] =
        json!("synthetic-private-configuration-value");
    doc = save(&mut ws, &doc, 2);
    assert_eq!(ws.audit.len(), 2);
    let event = &ws.audit[1];
    assert_eq!(event["action"], "workflow.updated");
    assert_eq!(event["changes"][0]["nodeId"], "hello");
    assert!(
        event["changes"][0]["description"]
            .as_str()
            .unwrap()
            .contains("configuration")
    );
    assert!(
        !ws.audit_page(0, 100)
            .to_string()
            .contains("synthetic-private-configuration-value")
    );
    ws.handle("GET", "/api/workspace", b"", 3, "").unwrap();
    for mode in ["simulation", "run"] {
        ws.handle("POST","/api/runs",json!({"workflowId":"01-hello","mode":mode,"input":{"payload":"synthetic-private-payload"}}).to_string().as_bytes(),3,mode).unwrap();
    }
    ws.alarm(10).unwrap();
    assert_eq!(ws.audit.len(), 2);
    assert!(
        !ws.audit_page(0, 100)
            .to_string()
            .contains("synthetic-private-payload")
    );
    let (_, published) = ws
        .handle(
            "POST",
            "/api/workflows/01-hello/publish",
            json!({"expectedVersion":doc["version"]})
                .to_string()
                .as_bytes(),
            4,
            "",
        )
        .unwrap();
    ws.handle(
        "POST",
        "/api/workflows/01-hello/publish",
        json!({"expectedVersion":published["version"]})
            .to_string()
            .as_bytes(),
        5,
        "",
    )
    .unwrap();
    assert_eq!(ws.audit.len(), 3);
    assert_eq!(ws.audit[2]["action"], "workflow.published");
    assert_eq!(ws.audit[2]["after"]["publishedRevision"], 1);
    assert_eq!(ws.audit_recent_page(0, 1)["items"][0]["id"], 3);
    assert_eq!(ws.audit_recent_page(3, 1)["items"][0]["id"], 2);
}
#[test]
fn legacy_run_and_transport_noise_is_hidden_without_fabricating_details() {
    let mut ws = Workspace::default();
    ws.audit = vec![
        json!({"id":1,"action":"runs.started","actor":"scheduler"}),
        json!({"id":2,"action":"workflows_get","actor":"agent"}),
        json!({"id":3,"action":"POST","actor":"browser","before":{"version":1},"after":{"version":2}}),
    ];
    let page = ws.audit_page(0, 100);
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["items"][0]["id"], 3);
    assert!(page["items"][0].get("changes").is_none());
}
