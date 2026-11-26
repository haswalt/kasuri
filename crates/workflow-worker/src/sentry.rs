//! Deliver authorized project reports without storing responses or credentials.
use connector_sdk::{FailureCode, Outcome, Task};
use connectors_sentry::{Envelope, failure};
use futures_util::future::{Either, select};
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeMap, time::Duration};
use worker::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    dsn: String,
    workspaces: Vec<String>,
}

fn projects(env: &Env) -> std::result::Result<BTreeMap<String, Project>, FailureCode> {
    let raw = env
        .secret("SENTRY_PROJECTS")
        .map(|value| value.to_string())
        .or_else(|_| env.var("SENTRY_PROJECTS").map(|value| value.to_string()))
        .unwrap_or_default();
    if raw.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    serde_json::from_str(&raw).map_err(|_| FailureCode::InvalidConfig)
}

fn permitted(project: &Project, workspace: &str) -> bool {
    project.workspaces.iter().any(|id| id == workspace)
}

pub(crate) fn list_projects(env: &Env, workspace: &str) -> Result<Response> {
    let Ok(projects) = projects(env) else {
        return crate::api_error(500, "Invalid Sentry project configuration");
    };
    let aliases: Vec<_> = projects
        .into_iter()
        .filter(|(_, project)| permitted(project, workspace))
        .map(|(alias, _)| alias)
        .collect();
    Response::from_json(&json!({"projects":aliases}))
}

fn envelope(task: &Task, env: &Env, workspace: &str) -> std::result::Result<Envelope, FailureCode> {
    let alias = connectors_sentry::validate(&task.node.config)?;
    let projects = projects(env)?;
    let project = projects
        .get(alias)
        .filter(|project| permitted(project, workspace))
        .ok_or(FailureCode::EgressDenied)?;
    connectors_sentry::prepare(task, &project.dsn, crate::now())
}

async fn send(report: &Envelope, env: &Env, signal: &AbortSignal) -> Result<Outcome> {
    let hosts = env
        .var("HTTP_ALLOWED_HOSTS")
        .map(|value| value.to_string())
        .unwrap_or_default();
    if !connectors_http::allowed_url(&report.url, &hosts)
        || !connectors_http::public_url_destination(&report.url)
        || !crate::http::public_dns(&report.url, signal)
            .await
            .unwrap_or(false)
    {
        return Ok(failure(FailureCode::EgressDenied));
    }
    let headers = Headers::new();
    headers.set("Content-Type", "application/x-sentry-envelope")?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_redirect(RequestRedirect::Manual)
        .with_body(Some(report.body.as_str().into()));
    let response = Fetch::Request(Request::new_with_init(report.url.as_str(), &init)?)
        .send_with_signal(signal)
        .await?;
    if (200..300).contains(&response.status_code()) {
        Ok(Outcome::success(
            json!({"status":"accepted","eventId":report.event_id}),
        ))
    } else {
        Ok(failure(FailureCode::External))
    }
}

pub(crate) async fn execute(task: &Task, env: &Env, workspace: &str) -> Outcome {
    let report = match envelope(task, env, workspace) {
        Ok(report) => report,
        Err(code) => return failure(code),
    };
    let remaining = task.deadline_ms.saturating_sub(crate::now());
    if remaining == 0 {
        return failure(FailureCode::Timeout);
    }
    let controller = AbortController::default();
    let signal = controller.signal();
    let outcome = match select(
        Box::pin(send(&report, env, &signal)),
        Box::pin(Delay::from(Duration::from_millis(remaining))),
    )
    .await
    {
        Either::Left((result, _)) => {
            result.unwrap_or_else(|_| failure(FailureCode::OutcomeUnknown))
        }
        Either::Right(_) => failure(FailureCode::Timeout),
    };
    controller.abort();
    outcome
}
