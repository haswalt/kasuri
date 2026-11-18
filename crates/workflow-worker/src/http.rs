//! Outbound HTTP execution. Checkpoint dispatch before calling this function.
use connector_sdk::{FailureCode, Outcome, Task};
use connectors_http::{MAX_RESPONSE_BYTES, RequestSpec, failure};
use futures_util::{
    StreamExt,
    future::{Either, select},
};
use std::time::Duration;
use worker::*;
async fn request(task: &Task, spec: &RequestSpec, signal: &AbortSignal) -> Result<Outcome> {
    let headers = Headers::new();
    for (name, value) in &spec.headers {
        headers.set(name, value)?;
    }
    headers.set("Idempotency-Key", &task.idempotency_key)?;
    let mut init = RequestInit::new();
    init.with_method(Method::from(spec.method.clone()))
        .with_headers(headers)
        .with_redirect(RequestRedirect::Manual);
    if let Some(body) = &spec.body {
        init.with_body(Some(body.as_str().into()));
    }
    let mut response = Fetch::Request(Request::new_with_init(&spec.url, &init)?)
        .send_with_signal(signal)
        .await?;
    let status = response.status_code();
    if !(200..300).contains(&status) {
        return Ok(connectors_http::response(task, spec, status, None, &[]));
    }
    if response
        .headers()
        .get("Content-Length")?
        .and_then(|s| s.parse::<usize>().ok())
        .is_some_and(|size| size > MAX_RESPONSE_BYTES)
    {
        return Ok(failure(FailureCode::BudgetExceeded, false));
    }
    let content_type = response.headers().get("Content-Type")?;
    let mut bytes = Vec::new();
    if spec.method != "HEAD" && status != 204 {
        let mut stream = response.stream()?;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Ok(failure(FailureCode::BudgetExceeded, false));
            }
            bytes.extend(chunk);
        }
    }
    Ok(connectors_http::response(
        task,
        spec,
        status,
        content_type.as_deref(),
        &bytes,
    ))
}
pub async fn execute(task: &Task, env: &Env) -> Outcome {
    let spec = match connectors_http::validate(task) {
        Ok(v) => v,
        Err(code) => return failure(code, false),
    };
    let hosts = env
        .var("HTTP_ALLOWED_HOSTS")
        .map(|v| v.to_string())
        .unwrap_or_default();
    if !connectors_http::allowed(&spec, &hosts) {
        return failure(FailureCode::EgressDenied, false);
    }
    let remaining = task.deadline_ms.saturating_sub(Date::now().as_millis());
    if remaining == 0 {
        return failure(FailureCode::Timeout, connectors_http::is_read(task));
    }
    let controller = AbortController::default();
    let signal = controller.signal();
    let fetch = Box::pin(request(task, &spec, &signal));
    let timeout = Box::pin(Delay::from(Duration::from_millis(remaining)));
    let outcome = match select(fetch, timeout).await {
        Either::Left((result, _)) => result.unwrap_or_else(|_| {
            failure(
                if connectors_http::is_read(task) {
                    FailureCode::External
                } else {
                    FailureCode::OutcomeUnknown
                },
                connectors_http::is_read(task),
            )
        }),
        Either::Right((_, _)) => failure(FailureCode::Timeout, connectors_http::is_read(task)),
    };
    // Also cancel the body stream on oversize or any early return.
    controller.abort();
    outcome
}
