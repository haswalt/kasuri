use connector_sdk::{Connector, Failure, FailureCode, Outcome, Task};
use connectors_core::CoreConnector;
use serde_json::{Value, json};
use workflow_engine::{Dispatch, Limits, NodeState, Run, RunStatus};
use workflow_schema::Workflow;

fn graph() -> Workflow {
    Workflow::from_json(include_str!("../../../examples/basic-workflow.json")).unwrap()
}
fn run(workflow: Workflow) -> Run {
    Run::new(
        "test-run",
        workflow,
        json!({"priority":"urgent","title":"Test"}),
        0,
        Limits::default(),
    )
    .unwrap()
}
fn task(run: &mut Run, now: u64) -> Task {
    match run.next(now).unwrap() {
        Dispatch::Task(task) => *task,
        other => panic!("Expected task, got {other:?}"),
    }
}
fn complete(run: &mut Run, now: u64) -> Task {
    let task = task(run, now);
    run.complete(&task.id, CoreConnector.execute(&task), now)
        .unwrap();
    task
}
fn finish(run: &mut Run) {
    let mut now = 0;
    for _ in 0..100 {
        match run.next(now).unwrap() {
            Dispatch::Task(task) => run
                .complete(&task.id, CoreConnector.execute(&task), now)
                .unwrap(),
            Dispatch::SleepUntil(at) => now = at,
            Dispatch::Finished(RunStatus::Succeeded) => return,
            other => panic!("Unexpected {other:?}"),
        }
    }
    panic!("Engine failed to terminate")
}
fn single() -> Workflow {
    Workflow::from_json(r#"{"schemaVersion":1,"id":"one","name":"One","nodes":[{"id":"start","type":"Manual Trigger","label":"Start","config":{}}],"edges":[]}"#).unwrap()
}

#[test]
fn branches_waits_and_merges_with_recovery_between_every_transition() {
    let mut run = run(graph());
    for _ in 0..3 {
        complete(&mut run, 0);
        run = Run::restore(&run.checkpoint().unwrap()).unwrap();
    }
    complete(&mut run, 0); // Wait node suspends instead of executing again.
    assert_eq!(run.next(0).unwrap(), Dispatch::SleepUntil(1000));
    run = Run::restore(&run.checkpoint().unwrap()).unwrap();
    let done = complete(&mut run, 1000);
    assert_eq!(done.node.id, "done");
    assert_eq!(done.input["wait"]["title"], "Test");
    assert_eq!(
        run.next(1000).unwrap(),
        Dispatch::Finished(RunStatus::Succeeded)
    );
    assert_eq!(run.dispatches(), 5);
}
#[test]
fn untaken_branch_is_skipped_and_join_receives_only_active_parents() {
    let mut workflow = graph();
    workflow.nodes[2]
        .config
        .insert("equals".into(), json!("normal"));
    let mut run = run(workflow);
    finish(&mut run);
    assert_eq!(run.nodes()["wait"], NodeState::Skipped);
    match &run.nodes()["done"] {
        NodeState::Succeeded { output, .. } => {
            assert!(output.get("condition").is_some());
            assert!(output.get("wait").is_none());
        }
        other => panic!("{other:?}"),
    }
}
#[test]
fn in_flight_task_is_recovered_with_same_identity_without_redispatch_count() {
    let mut run = run(single());
    let original = task(&mut run, 0);
    let mut restored = Run::restore(&run.checkpoint().unwrap()).unwrap();
    assert_eq!(original, task(&mut restored, 10));
    assert_eq!(restored.dispatches(), 1);
}
#[test]
fn retries_back_off_and_preserve_effect_key_but_change_attempt_identity() {
    let mut workflow = single();
    workflow.nodes[0].retry.max_attempts = 3;
    let mut run = run(workflow);
    let first = task(&mut run, 0);
    let failure = Outcome::Failed {
        failure: Failure {
            code: FailureCode::External,
            retryable: true,
        },
    };
    run.complete(&first.id, failure.clone(), 0).unwrap();
    assert_eq!(run.next(999).unwrap(), Dispatch::SleepUntil(1000));
    let second = task(&mut run, 1000);
    assert_eq!(first.idempotency_key, second.idempotency_key);
    assert_ne!(first.id, second.id);
    assert_eq!(second.attempt, 2);
    run.complete(&second.id, failure.clone(), 1000).unwrap();
    assert_eq!(run.next(1000).unwrap(), Dispatch::SleepUntil(3000));
    let third = task(&mut run, 3000);
    run.complete(&third.id, failure, 3000).unwrap();
    assert_eq!(run.status(), RunStatus::Failed);
    assert_eq!(run.failure(), Some(&FailureCode::External));
}
#[test]
fn exhausted_or_permanent_failures_route_to_explicit_error_edges() {
    let mut workflow = single();
    let mut handler = workflow.nodes[0].clone();
    handler.id = "handler".into();
    handler.kind = "Error Handler".into();
    workflow.nodes.push(handler);
    workflow.edges.push(
        serde_json::from_value(
            json!({"id":"error","source":"start","target":"handler","sourcePort":"error"}),
        )
        .unwrap(),
    );
    let mut run = run(workflow);
    let first = task(&mut run, 0);
    run.complete(
        &first.id,
        Outcome::Failed {
            failure: Failure::permanent(FailureCode::External),
        },
        0,
    )
    .unwrap();
    let handler = complete(&mut run, 0);
    assert_eq!(handler.input, json!({"error":{"code":"external"}}));
    assert_eq!(
        run.next(0).unwrap(),
        Dispatch::Finished(RunStatus::Succeeded)
    );
}
#[test]
fn duplicate_and_stale_results_are_rejected_without_mutating_state() {
    let mut run = run(single());
    let first = complete(&mut run, 0);
    let checkpoint = run.checkpoint().unwrap();
    assert!(
        run.complete(&first.id, Outcome::success(Value::Null), 10)
            .is_err()
    );
    assert_eq!(run.checkpoint().unwrap(), checkpoint);
}
#[test]
fn pause_accepts_in_flight_completion_and_blocks_new_dispatch() {
    let mut run = run(graph());
    let first = task(&mut run, 0);
    run.pause().unwrap();
    run.complete(&first.id, Outcome::success(json!({})), 0)
        .unwrap();
    assert_eq!(run.next(0).unwrap(), Dispatch::Paused);
    run = Run::restore(&run.checkpoint().unwrap()).unwrap();
    run.resume().unwrap();
    assert_eq!(task(&mut run, 0).node.id, "map");
}
#[test]
fn cancellation_is_terminal_and_rejects_late_completions() {
    let mut run = run(single());
    let first = task(&mut run, 0);
    run.cancel().unwrap();
    assert_eq!(
        run.next(1).unwrap(),
        Dispatch::Finished(RunStatus::Cancelled)
    );
    assert!(
        run.complete(&first.id, Outcome::success(Value::Null), 1)
            .is_err()
    );
    assert!(run.resume().is_err());
}
#[test]
fn deadline_caps_wait_wakeup_and_fails_run() {
    let mut run = Run::new(
        "deadline",
        graph(),
        json!({"priority":"urgent","title":"Test"}),
        0,
        Limits {
            max_duration_ms: 100,
            ..Limits::default()
        },
    )
    .unwrap();
    for _ in 0..4 {
        complete(&mut run, 0);
    }
    assert_eq!(run.next(0).unwrap(), Dispatch::SleepUntil(100));
    assert_eq!(
        run.next(100).unwrap(),
        Dispatch::Finished(RunStatus::Failed)
    );
    assert_eq!(run.failure(), Some(&FailureCode::DeadlineExceeded));
}
#[test]
fn dispatch_budget_prevents_next_side_effect() {
    let mut run = Run::new(
        "budget",
        graph(),
        json!({}),
        0,
        Limits {
            max_dispatches: 1,
            ..Limits::default()
        },
    )
    .unwrap();
    complete(&mut run, 0);
    assert_eq!(run.next(0).unwrap(), Dispatch::Finished(RunStatus::Failed));
    assert_eq!(run.failure(), Some(&FailureCode::BudgetExceeded));
}
#[test]
fn oversized_outputs_and_invalid_ports_are_rejected_atomically() {
    let mut run = run(single());
    let first = task(&mut run, 0);
    let before = run.checkpoint().unwrap();
    assert!(
        run.complete(&first.id, Outcome::success(json!("x".repeat(70_000))), 0)
            .is_err()
    );
    assert!(
        run.complete(
            &first.id,
            Outcome::Complete {
                output: Value::Null,
                port: "bogus".into()
            },
            0
        )
        .is_err()
    );
    assert_eq!(run.checkpoint().unwrap(), before);
}
#[test]
fn backwards_clock_is_rejected_atomically() {
    let mut run = run(single());
    task(&mut run, 10);
    let before = run.checkpoint().unwrap();
    assert!(run.next(9).is_err());
    assert_eq!(run.checkpoint().unwrap(), before);
}
#[test]
fn malformed_checkpoints_are_rejected() {
    let mut run = run(single());
    task(&mut run, 0);
    let checkpoint = run.checkpoint().unwrap();
    let mut value: Value = serde_json::from_str(&checkpoint).unwrap();
    value["checkpointVersion"] = json!(99);
    assert!(Run::restore(&value.to_string()).is_err());
    value["checkpointVersion"] = json!(1);
    value["nodes"]["start"]["task"]["attempt"] = json!(0);
    assert!(Run::restore(&value.to_string()).is_err());
    value["nodes"] = json!({});
    assert!(Run::restore(&value.to_string()).is_err());
}
#[test]
fn graph_storage_order_does_not_need_to_be_topological() {
    let mut workflow = graph();
    workflow.nodes.reverse();
    let mut run = run(workflow);
    finish(&mut run);
    assert_eq!(run.status(), RunStatus::Succeeded);
}
#[test]
fn ui_exports_are_accepted_with_default_node_version_retry_and_port() {
    let workflow = Workflow::from_json(r#"{"schemaVersion":1,"id":"ui","name":"UI export","status":"draft","updatedAt":"2026-11-18","revision":0,"description":"","nodes":[{"id":"a","type":"Manual Trigger","label":"Start","config":{}},{"id":"b","type":"Map / Transform","label":"Map","config":{"mapping":"{\"name\":{\"$input\":\"/title\"}}"}}],"edges":[{"id":"e","source":"a","target":"b"}]}"#).unwrap();
    assert_eq!(workflow.nodes[0].version, 1);
    let mut run = run(workflow);
    finish(&mut run);
    match &run.nodes()["b"] {
        NodeState::Succeeded { output, .. } => assert_eq!(output, &json!({"name":"Test"})),
        other => panic!("{other:?}"),
    }
}

#[test]
fn abandoned_tasks_timeout_on_next_tick_and_retry() {
    let mut workflow = single();
    workflow.nodes[0].timeout_ms = 10;
    workflow.nodes[0].retry.max_attempts = 2;
    let mut run = run(workflow);
    let first = task(&mut run, 0);
    assert_eq!(first.deadline_ms, 10);
    assert_eq!(run.next(10).unwrap(), Dispatch::SleepUntil(1010));
    let second = task(&mut run, 1010);
    assert!(
        run.complete(&first.id, Outcome::success(Value::Null), 1010)
            .is_err()
    );
    run.complete(&second.id, Outcome::success(json!("ok")), 1011)
        .unwrap();
    assert_eq!(
        run.next(1011).unwrap(),
        Dispatch::Finished(RunStatus::Succeeded)
    );
}
#[test]
fn late_success_cannot_override_a_task_timeout() {
    let mut workflow = single();
    workflow.nodes[0].timeout_ms = 10;
    let mut run = run(workflow);
    let first = task(&mut run, 0);
    run.complete(&first.id, Outcome::success(Value::Null), 10)
        .unwrap();
    assert_eq!(run.status(), RunStatus::Failed);
    assert_eq!(run.failure(), Some(&FailureCode::Timeout));
}
#[test]
fn effect_keys_do_not_collide_for_ids_containing_separators() {
    let mut workflow = single();
    workflow.nodes[0].id = "b/c".into();
    let mut first = Run::new("a", workflow.clone(), Value::Null, 0, Limits::default()).unwrap();
    workflow.nodes[0].id = "c".into();
    let mut second = Run::new("a/b", workflow, Value::Null, 0, Limits::default()).unwrap();
    assert_ne!(
        task(&mut first, 0).idempotency_key,
        task(&mut second, 0).idempotency_key
    );
}
