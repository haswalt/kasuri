use serde_json::json;
use workflow_schema::{RetryPolicy, Workflow};
fn graph() -> Workflow {
    Workflow::from_json(include_str!("../../../examples/basic-workflow.json")).unwrap()
}
#[test]
fn rejects_cycles_dangling_edges_and_disconnected_nodes() {
    let mut w = graph();
    w.edges.push(
        serde_json::from_value(json!({"id":"cycle","source":"done","target":"map"})).unwrap(),
    );
    assert!(w.validate().is_err());
    let mut w = graph();
    w.edges[0].source = "missing".into();
    assert!(w.validate().is_err());
    let mut w = graph();
    w.edges.clear();
    assert!(w.validate().is_err());
}
#[test]
fn requires_unique_ids_single_trigger_and_valid_ports() {
    let mut w = graph();
    w.nodes[1].id = "start".into();
    assert!(w.validate().is_err());
    let mut w = graph();
    w.nodes[1].kind = "Webhook".into();
    assert!(w.validate().is_err());
    let mut w = graph();
    w.edges[2].source_port = "success".into();
    assert!(w.validate().is_err());
    let mut w = graph();
    w.edges[1].id = w.edges[0].id.clone();
    assert!(w.validate().is_err());
    let mut w = graph();
    let mut edge = w.edges[0].clone();
    edge.id = "duplicate".into();
    w.edges.push(edge);
    assert!(w.validate().is_err());
}
#[test]
fn rejects_schema_mismatch_and_unbounded_retry_policy() {
    let mut w = graph();
    w.schema_version = 3;
    assert!(w.validate().is_err());
    let mut w = graph();
    w.nodes[0].retry.max_attempts = 100;
    assert!(w.validate().is_err());
    let mut w = graph();
    w.nodes[0].retry.max_attempts = 0;
    assert!(w.validate().is_err());
    let mut w = graph();
    w.nodes[0].timeout_ms = 0;
    assert!(w.validate().is_err());
}
#[test]
fn enforces_node_and_config_budgets() {
    let mut w = graph();
    w.nodes[0]
        .config
        .insert("large".into(), json!("x".repeat(70_000)));
    assert!(w.validate().is_err());
    let mut w = graph();
    w.nodes = vec![w.nodes[0].clone(); 101];
    assert!(w.validate().is_err());
    assert!(Workflow::from_json(&"x".repeat(1_048_577)).is_err());
}
#[test]
fn backoff_saturates_without_overflow() {
    let policy = RetryPolicy {
        max_attempts: 10,
        delay_ms: u64::MAX / 2,
        max_delay_ms: u64::MAX,
    };
    assert_eq!(policy.backoff_ms(1), u64::MAX / 2);
    assert_eq!(policy.backoff_ms(100), u64::MAX);
    assert_eq!(RetryPolicy::default().backoff_ms(10), 60_000);
}
