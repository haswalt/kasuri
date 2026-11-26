use connector_sdk::{Connector, Failure, FailureCode, Outcome, Task};
use connectors_core::CoreConnector;
use serde_json::{Value, json};
use workflow_engine::{Dispatch, Limits, Run, RunStatus};
use workflow_schema::Workflow;
fn graph(policy: &str) -> Workflow {
    serde_json::from_value(json!({"schemaVersion":1,"id":"parallel","name":"Parallel","nodes":[{"id":"start","type":"Manual Trigger","label":"Start"},{"id":"split","type":"Split / Parallel","label":"Parallel","config":{"concurrency":2}},{"id":"a","type":"HTTP Request","label":"A"},{"id":"b","type":"HTTP Request","label":"B"},{"id":"join","type":"Join","label":"Join","config":{"policy":policy}}],"edges":[{"id":"s","source":"start","target":"split"},{"id":"a","source":"split","target":"a"},{"id":"b","source":"split","target":"b"},{"id":"aj","source":"a","target":"join"},{"id":"bj","source":"b","target":"join"}]})).unwrap()
}
fn task(run: &mut Run, now: u64) -> Task {
    match run.next_parallel(now).unwrap() {
        Dispatch::Task(task) => *task,
        other => panic!("{other:?}"),
    }
}
fn prefix(run: &mut Run) {
    for _ in 0..2 {
        let task = task(run, 0);
        run.complete(&task.id, CoreConnector.execute(&task), 0)
            .unwrap();
    }
}
#[test]
fn concurrent_tasks_restore_and_join_in_stable_parent_order() {
    let mut run = Run::new("run", graph("all"), json!({}), 0, Limits::default()).unwrap();
    prefix(&mut run);
    let a = task(&mut run, 0);
    let b = task(&mut run, 0);
    assert_ne!(a.id, b.id);
    assert!(matches!(
        run.next_parallel(0).unwrap(),
        Dispatch::SleepUntil(_)
    ));
    run = Run::restore(&run.checkpoint().unwrap()).unwrap();
    run.complete(&b.id, Outcome::success(json!({"branch":"b"})), 10)
        .unwrap();
    assert!(matches!(
        run.next_parallel(10).unwrap(),
        Dispatch::SleepUntil(_)
    ));
    run.complete(&a.id, Outcome::success(json!({"branch":"a"})), 20)
        .unwrap();
    let join = task(&mut run, 20);
    assert_eq!(join.input, json!({"a":{"branch":"a"},"b":{"branch":"b"}}));
    run.complete(&join.id, CoreConnector.execute(&join), 20)
        .unwrap();
    assert_eq!(
        run.next_parallel(20).unwrap(),
        Dispatch::Finished(RunStatus::Succeeded)
    );
    assert!(
        run.complete(&a.id, Outcome::success(Value::Null), 20)
            .is_err()
    );
}
#[test]
fn collect_reports_failed_branches_while_all_fails_fast() {
    for policy in ["collect", "all"] {
        let mut run = Run::new("run", graph(policy), json!({}), 0, Limits::default()).unwrap();
        prefix(&mut run);
        let a = task(&mut run, 0);
        let b = task(&mut run, 0);
        run.complete(
            &a.id,
            Outcome::Failed {
                failure: Failure::permanent(FailureCode::External),
            },
            1,
        )
        .unwrap();
        if policy == "all" {
            assert_eq!(run.status(), RunStatus::Failed);
            continue;
        }
        run.complete(&b.id, Outcome::success(json!(2)), 2).unwrap();
        let join = task(&mut run, 2);
        assert_eq!(
            join.input["a"],
            json!({"status":"failed","error":"external"})
        );
        assert_eq!(join.input["b"], json!({"status":"succeeded","output":2}));
        run.complete(&join.id, CoreConnector.execute(&join), 2)
            .unwrap();
        assert_eq!(
            run.next_parallel(2).unwrap(),
            Dispatch::Finished(RunStatus::Succeeded)
        );
        Run::restore(&run.checkpoint().unwrap()).unwrap();
    }
}
#[test]
fn pause_retains_sibling_outcomes_without_dispatching_join() {
    let mut run = Run::new("run", graph("all"), json!({}), 0, Limits::default()).unwrap();
    prefix(&mut run);
    let a = task(&mut run, 0);
    let b = task(&mut run, 0);
    run.pause().unwrap();
    run.complete(&a.id, Outcome::success(json!(1)), 1).unwrap();
    run.complete(&b.id, Outcome::success(json!(2)), 2).unwrap();
    assert_eq!(run.next_parallel(2).unwrap(), Dispatch::Paused);
    run.resume().unwrap();
    assert_eq!(task(&mut run, 2).node.id, "join");
}

#[test]
fn concurrency_bound_holds_while_retrying_only_the_failed_branch() {
    let mut graph = graph("all");
    let mut third = graph.nodes[3].clone();
    third.id = "c".into();
    graph.nodes.insert(4, third);
    graph
        .edges
        .push(serde_json::from_value(json!({"id":"c","source":"split","target":"c"})).unwrap());
    graph
        .edges
        .push(serde_json::from_value(json!({"id":"cj","source":"c","target":"join"})).unwrap());
    graph.nodes[2].retry.max_attempts = 2;
    graph.nodes[2].retry.delay_ms = 10;
    let mut run = Run::new("run", graph, json!({}), 0, Limits::default()).unwrap();
    prefix(&mut run);
    let a = task(&mut run, 0);
    let b = task(&mut run, 0);
    assert!(matches!(
        run.next_parallel(0).unwrap(),
        Dispatch::SleepUntil(_)
    ));
    run.complete(
        &a.id,
        Outcome::Failed {
            failure: Failure::new(FailureCode::External, true),
        },
        1,
    )
    .unwrap();
    let c = task(&mut run, 1);
    assert_eq!(c.node.id, "c");
    run.complete(&b.id, Outcome::success(json!(2)), 2).unwrap();
    let retry = task(&mut run, 11);
    assert_eq!(retry.node.id, "a");
    assert_eq!(retry.attempt, 2);
    assert_eq!(retry.idempotency_key, a.idempotency_key);
    run.complete(&retry.id, Outcome::success(json!(1)), 12)
        .unwrap();
    run.complete(&c.id, Outcome::success(json!(3)), 13).unwrap();
    let join = task(&mut run, 13);
    assert_eq!(join.input, json!({"a":1,"b":2,"c":3}));
}
