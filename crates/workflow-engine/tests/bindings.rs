use connector_sdk::{Connector, Failure, FailureCode, Outcome};
use connectors_core::CoreConnector;
use serde_json::{Value, json};
use workflow_engine::{Dispatch, Limits, Run, RunStatus};
use workflow_schema::Workflow;
fn workflow() -> Workflow {
    serde_json::from_value(json!({"schemaVersion":2,"id":"bindings","name":"Bindings","nodes":[{"id":"start","type":"Manual Trigger","label":"Start"},{"id":"source","type":"Map / Transform","label":"Source","config":{"mapping":{"orderId":"ORD-1001","amount":47449}}},{"id":"wait","type":"Wait / Delay","label":"Wait","config":{"delayMs":100}},{"id":"http","type":"HTTP Request","label":"HTTP","config":{"url":"https://example.com","method":"POST","body":{"$input":""}},"retry":{"maxAttempts":2,"delayMs":10,"maxDelayMs":10},"inputBindings":{"orderId":{"op":"node","nodeId":"source","path":"/orderId"},"total":{"op":"multiply","args":[{"op":"node","nodeId":"source","path":"/amount"},{"op":"literal","value":2}]},"original":{"op":"trigger","path":"/original"},"workflow":{"op":"workflow","path":"/id"}}}],"edges":[{"id":"a","source":"start","target":"source"},{"id":"b","source":"source","target":"wait"},{"id":"c","source":"wait","target":"http"}]})).unwrap()
}
#[test]
fn bindings_are_typed_persisted_and_stable_across_wait_restart_and_retry() {
    let mut run = Run::new(
        "run",
        workflow(),
        json!({"original":true}),
        0,
        Limits::default(),
    )
    .unwrap();
    let mut now = 0;
    let mut dispatched = None;
    for _ in 0..10 {
        run = Run::restore(&run.checkpoint().unwrap()).unwrap();
        match run.next(now).unwrap() {
            Dispatch::Task(task) if task.node.id == "http" => {
                dispatched = Some(*task);
                break;
            }
            Dispatch::Task(task) => run
                .complete(&task.id, CoreConnector.execute(&task), now)
                .unwrap(),
            Dispatch::SleepUntil(at) => now = at,
            other => panic!("{other:?}"),
        }
    }
    let task = dispatched.unwrap();
    assert_eq!(
        task.input,
        json!({"orderId":"ORD-1001","total":94898,"original":true,"workflow":"bindings"})
    );
    // Resolved bindings leave no ancestor outputs or trigger payload on the task.
    assert!(task.context.is_null(), "{}", task.context);
    assert!(
        !run.checkpoint()
            .unwrap()
            .to_string()
            .contains("\"context\":{")
    );
    let mut restored = Run::restore(&run.checkpoint().unwrap()).unwrap();
    let Dispatch::Task(same) = restored.next(now).unwrap() else {
        panic!()
    };
    assert_eq!(*same, task);
    restored
        .complete(
            &task.id,
            Outcome::Failed {
                failure: Failure {
                    code: FailureCode::External,
                    retryable: true,
                },
            },
            now,
        )
        .unwrap();
    now += 10;
    let Dispatch::Task(retry) = restored.next(now).unwrap() else {
        panic!()
    };
    assert_eq!(retry.input, task.input);
    assert_eq!(retry.idempotency_key, task.idempotency_key);
    assert_eq!(retry.node.config, task.node.config);
}
#[test]
fn missing_binding_fails_before_external_dispatch_and_can_follow_error_port() {
    let mut graph = workflow();
    let http = graph.nodes.iter_mut().find(|n| n.id == "http").unwrap();
    http.input_bindings = json!({"bad":{"op":"input","path":"/absent"}})
        .as_object()
        .unwrap()
        .clone();
    let handler: workflow_schema::Node =
        serde_json::from_value(json!({"id":"error","type":"Error Handler","label":"Error"}))
            .unwrap();
    graph.nodes.push(handler);
    graph.edges.push(
        serde_json::from_value(
            json!({"id":"e","source":"http","target":"error","sourcePort":"error"}),
        )
        .unwrap(),
    );
    let mut run = Run::new("run", graph, json!({}), 0, Limits::default()).unwrap();
    let mut now = 0;
    let mut handled = false;
    for _ in 0..15 {
        match run.next(now).unwrap() {
            Dispatch::Task(task) => {
                assert_ne!(
                    task.node.id, "http",
                    "A failed binding must not reach an external host"
                );
                handled |= task.node.id == "error";
                run.complete(&task.id, CoreConnector.execute(&task), now)
                    .unwrap()
            }
            Dispatch::SleepUntil(at) => now = at,
            Dispatch::Finished(status) => {
                assert_eq!(status, RunStatus::Succeeded);
                assert!(handled);
                return;
            }
            other => panic!("{other:?}"),
        }
    }
    panic!("Did not finish")
}
#[test]
fn new_bindings_require_format_two_and_cannot_read_private_host_context() {
    let mut graph = workflow();
    graph.schema_version = 1;
    assert!(graph.validate().is_err());
    graph.schema_version = 2;
    let mut value = serde_json::to_value(&graph).unwrap();
    value["nodes"][3]["inputBindings"] = json!({"secret":{"op":"workflow","path":"/credentials"}});
    let graph: Workflow = serde_json::from_value(value).unwrap();
    let mut run = Run::new("run", graph, Value::Null, 0, Limits::default()).unwrap();
    let mut now = 0;
    for _ in 0..10 {
        match run.next(now).unwrap() {
            Dispatch::Task(task) => {
                assert_ne!(task.node.id, "http");
                run.complete(&task.id, CoreConnector.execute(&task), now)
                    .unwrap()
            }
            Dispatch::SleepUntil(at) => now = at,
            Dispatch::Finished(status) => {
                assert_eq!(status, RunStatus::Failed);
                return;
            }
            other => panic!("{other:?}"),
        }
    }
    panic!("Did not finish")
}
