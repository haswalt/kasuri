use connector_sdk::{Connector, Outcome};
use connectors_core::CoreConnector;
use serde_json::{Value, json};
use workflow_engine::{Dispatch, Limits, NodeState, Run, RunStatus};
use workflow_schema::Workflow;
#[test]
fn expression_context_and_router_ports_survive_every_checkpoint() {
    let workflow = Workflow::from_json(include_str!(
        "../../../examples/workflows/26-expressions-router.json"
    ))
    .unwrap();
    let mut run = Run::new(
        "expression-run",
        workflow,
        json!({"request":"original"}),
        42,
        Limits::default(),
    )
    .unwrap();
    for _ in 0..30 {
        run = Run::restore(&run.checkpoint().unwrap()).unwrap();
        match run.next(42).unwrap() {
            Dispatch::Task(task) => {
                if task.node.id == "calculate" {
                    assert_eq!(task.context["nodes"]["source"]["price"], 12);
                    assert!(task.context["nodes"].get("small").is_none());
                    assert_eq!(task.context["run"]["startedAtMs"], 42);
                    let restored = Run::restore(&run.checkpoint().unwrap()).unwrap();
                    assert_eq!(restored, run);
                }
                run.complete(&task.id, CoreConnector.execute(&task), 42)
                    .unwrap();
            }
            Dispatch::Finished(RunStatus::Succeeded) => {
                assert!(matches!(run.nodes()["small"], NodeState::Skipped));
                return;
            }
            other => panic!("{other:?}"),
        }
    }
    panic!("Run did not finish")
}
#[test]
fn legacy_tasks_without_context_restore() {
    let workflow = Workflow::from_json(include_str!(
        "../../../examples/workflows/28-date-expressions.json"
    ))
    .unwrap();
    let mut run = Run::new("old", workflow, Value::Null, 0, Limits::default()).unwrap();
    let Dispatch::Task(task) = run.next(0).unwrap() else {
        panic!()
    };
    let mut checkpoint: Value = serde_json::from_str(&run.checkpoint().unwrap()).unwrap();
    checkpoint["nodes"]["start"]["task"]
        .as_object_mut()
        .unwrap()
        .remove("context");
    let mut restored = Run::restore(&checkpoint.to_string()).unwrap();
    restored
        .complete(&task.id, Outcome::success(Value::Null), 0)
        .unwrap();
}
