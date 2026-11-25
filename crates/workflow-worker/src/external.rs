//! Dispatch claimed network tasks from workspace, run objects and queue consumers.
use connector_sdk::{FailureCode, Outcome, Task};
use futures_util::future::{Either, select};
use serde_json::Value;
use std::time::Duration;
use worker::{Delay, Env};

pub(crate) async fn execute(task: &Task, env: &Env, workspace: &str) -> Outcome {
    if task.node.kind == connectors_email::SEND {
        return crate::email::send(task, env, workspace).await;
    }
    if task.node.kind != connectors_clairic::KIND {
        return crate::http::execute(task, env).await;
    }

    let prepared = match connectors_clairic::prepare(task) {
        Ok(prepared) => prepared,
        Err(code) => return connectors_clairic::failed(code),
    };
    let Ok(ai) = env.ai("AI") else {
        return connectors_clairic::failed(FailureCode::InvalidConfig);
    };
    let remaining = task.deadline_ms.saturating_sub(crate::now());
    if remaining == 0 {
        return connectors_clairic::failed(FailureCode::Timeout);
    }
    let inference = Box::pin(ai.run::<_, Value>(connectors_clairic::MODEL, prepared.request()));
    let timeout = Box::pin(Delay::from(Duration::from_millis(remaining)));
    match select(inference, timeout).await {
        Either::Left((Ok(response), _)) => prepared.complete(response),
        Either::Left((Err(_), _)) => connectors_clairic::failed(FailureCode::External),
        Either::Right(_) => connectors_clairic::failed(FailureCode::Timeout),
    }
}
