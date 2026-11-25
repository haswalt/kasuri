//! Outbound HTTP execution. Checkpoint dispatch before calling this function.
use connector_sdk::{FailureCode, Outcome, Task};
use connectors_http::{MAX_RESPONSE_BYTES, RequestSpec, failure};
use futures_util::{
    StreamExt,
    future::{Either, select},
};
use std::time::Duration;
use worker::*;
pub(crate) async fn public_dns(url: &Url, signal: &AbortSignal) -> Result<bool> {
    let Some(host) = url.host_str() else {
        return Ok(false);
    };
    if host
        .trim_matches(['[', ']'])
        .parse::<std::net::IpAddr>()
        .is_ok()
    {
        return Ok(true);
    }
    let mut found = false;
    for kind in ["A", "AAAA"] {
        let mut endpoint = Url::parse("https://cloudflare-dns.com/dns-query").unwrap();
        endpoint
            .query_pairs_mut()
            .append_pair("name", host)
            .append_pair("type", kind);
        let headers = Headers::new();
        headers.set("Accept", "application/dns-json")?;
        let mut init = RequestInit::new();
        init.with_headers(headers)
            .with_redirect(RequestRedirect::Manual);
        let mut response = Fetch::Request(Request::new_with_init(endpoint.as_str(), &init)?)
            .send_with_signal(signal)
            .await?;
        if response.status_code() != 200 {
            return Ok(false);
        }
        let mut body = Vec::new();
        let mut stream = response.stream()?;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if body.len() + chunk.len() > 16_384 {
                return Ok(false);
            }
            body.extend(chunk);
        }
        let value: serde_json::Value = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };
        if value["Status"] != 0 {
            return Ok(false);
        }
        if let Some(answers) = value["Answer"].as_array() {
            for answer in answers {
                if matches!(answer["type"].as_u64(), Some(1 | 28)) {
                    let Some(ip) = answer["data"].as_str().and_then(|s| s.parse().ok()) else {
                        return Ok(false);
                    };
                    if !connectors_http::public_ip(ip) {
                        return Ok(false);
                    }
                    found = true;
                }
            }
        }
    }
    Ok(found)
}
async fn prepare(
    task: &Task,
    spec: &RequestSpec,
    env: &Env,
    signal: &AbortSignal,
) -> Result<Outcome> {
    let private_hosts = env
        .var("HTTP_PRIVATE_HOSTS")
        .map(|v| v.to_string())
        .unwrap_or_default();
    if !connectors_http::private_opt_in(spec, &private_hosts)
        && (!connectors_http::public_destination(spec)
            || !public_dns(&Url::parse(&spec.url).unwrap(), signal)
                .await
                .unwrap_or(false))
    {
        return Ok(failure(FailureCode::EgressDenied, false));
    }
    let needs_secrets = spec.basic.is_some() || !spec.secret_headers.is_empty();
    let secrets = if needs_secrets {
        // Workspaces are otherwise anonymous browser capabilities. Never expose installation
        // credentials to arbitrary public workflow authors merely because they know an alias.
        let authentication = env
            .secret("BASIC_AUTH")
            .map(|v| v.to_string())
            .or_else(|_| env.var("BASIC_AUTH").map(|v| v.to_string()))
            .unwrap_or_default();
        if authentication.trim().is_empty() {
            return Ok(failure(FailureCode::EgressDenied, false));
        }
        let raw = env
            .secret("HTTP_SECRETS")
            .map(|v| v.to_string())
            .or_else(|_| env.var("HTTP_SECRETS").map(|v| v.to_string()))
            .unwrap_or_default();
        match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => return Ok(failure(FailureCode::MissingInput, false)),
        }
    } else {
        std::collections::BTreeMap::new()
    };
    let (headers, redactions) = match connectors_http::authorization_headers(spec, &secrets) {
        Ok(v) => v,
        Err(code) => return Ok(failure(code, false)),
    };
    let mut authorized = spec.clone();
    authorized.headers = headers;
    let mut outcome = request(task, &authorized, signal).await?;
    if let Outcome::Complete { output, .. } = &mut outcome {
        connectors_http::redact(output, &redactions);
        if output.to_string().len() > 65_536 {
            return Ok(failure(FailureCode::BudgetExceeded, false));
        }
    }
    Ok(outcome)
}
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
    let fetch = Box::pin(prepare(task, &spec, env, &signal));
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
