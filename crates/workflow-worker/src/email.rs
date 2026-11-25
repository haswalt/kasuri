//! Route Cloudflare Email events to a published workspace workflow.
use crate::{WorkflowWorkspace, api_error, internal, now};
use connector_sdk::{FailureCode, Outcome, Task};
use connectors_email::{Inbound, MessageParser};
use futures_util::{
    StreamExt,
    future::{Either, select},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeMap, time::Duration};
use worker::*;

const INGRESS: &str = "/internal/email";
const MAX_RAW_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Route {
    workspace_id: String,
    workflow_id: String,
}
fn routes(env: &Env) -> Result<BTreeMap<String, Route>> {
    let setting = env
        .secret("EMAIL_ROUTES")
        .or_else(|_| env.var("EMAIL_ROUTES"))
        .map(|value| value.to_string())
        .unwrap_or_default();
    if setting.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    serde_json::from_str(&setting).map_err(|_| internal("Invalid EMAIL_ROUTES"))
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Delivery {
    workflow_id: String,
    mail: Inbound,
}

#[event(email)]
pub async fn receive(message: ForwardableEmailMessage, env: Env, _ctx: Context) -> Result<()> {
    let recipient = message.to().to_ascii_lowercase();
    let mut routes = routes(&env)?;
    let Some(route) = routes.remove(&recipient) else {
        message.set_reject("No workflow is configured for this address");
        return Ok(());
    };
    let namespace = env.durable_object("WORKSPACES")?;
    let stub = namespace.id_from_string(&route.workspace_id)?.get_stub()?;

    let raw_size = message.raw_size();
    if raw_size > MAX_RAW_BYTES as f64 {
        message.set_reject("Message exceeds the Kasuri email size limit");
        return Ok(());
    }
    let mut raw = Vec::new();
    let mut stream = message.raw_byte_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if raw.len().saturating_add(chunk.len()) > MAX_RAW_BYTES {
            message.set_reject("Message exceeds the Kasuri email size limit");
            return Ok(());
        }
        raw.extend(chunk);
    }
    let Some(parsed) = MessageParser::new().parse(&raw) else {
        message.set_reject("Invalid email message");
        return Ok(());
    };
    let delivery = Delivery {
        workflow_id: route.workflow_id,
        mail: connectors_email::parse(message.from(), message.to(), &parsed),
    };
    let body = serde_json::to_string(&delivery)?;
    // The email is the first node's output and must fit the existing run value limit.
    if body.len() > 65_536 {
        message.set_reject("Decoded email is too large for a workflow input");
        return Ok(());
    }
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(body.into()));
    let response = stub
        .fetch_with_request(Request::new_with_init(
            &format!("https://workspace{INGRESS}"),
            &init,
        )?)
        .await?;
    match response.status_code() {
        202 => Ok(()),
        400 | 404 => {
            message.set_reject("No active email workflow for this message");
            Ok(())
        }
        _ => Err(internal("Could not accept email workflow run")),
    }
}

impl WorkflowWorkspace {
    pub(crate) async fn incoming_email(&self, mut req: Request) -> Result<Response> {
        let delivery: Delivery = req.json().await?;
        let mut workspace = self.load()?;
        let run_id = self
            .env
            .durable_object("WORKSPACES")?
            .unique_id()?
            .to_string();
        if let Err(error) = workspace.receive_email(
            &delivery.workflow_id,
            serde_json::to_value(delivery.mail)?,
            now(),
            &run_id,
        ) {
            return api_error(error.status, &error.message);
        }
        self.save(&mut workspace).await?;
        Ok(Response::empty()?.with_status(202))
    }
}

pub(crate) async fn send(task: &Task, env: &Env, workspace_id: &str) -> Outcome {
    use connectors_email::{failed, prepare, valid_address};
    let Ok(routes) = routes(env) else {
        return failed(FailureCode::InvalidConfig);
    };
    if !routes
        .values()
        .any(|route| route.workspace_id == workspace_id)
    {
        return failed(FailureCode::EgressDenied);
    }
    let outbound = match prepare(task) {
        Ok(outbound) => outbound,
        Err(code) => return failed(code),
    };
    let Ok(from) = env.var("EMAIL_FROM").map(|value| value.to_string()) else {
        return failed(FailureCode::InvalidConfig);
    };
    let Ok(binding) = env.send_email("EMAIL") else {
        return failed(FailureCode::InvalidConfig);
    };
    if !valid_address(&from) {
        return failed(FailureCode::InvalidConfig);
    }
    let remaining = task.deadline_ms.saturating_sub(now());
    if remaining == 0 {
        return failed(FailureCode::Timeout);
    }
    let mut builder =
        SendEmailBuilder::builder(&from, outbound.to, outbound.subject).text(outbound.text);
    if let Some(message_id) = outbound.in_reply_to {
        use worker::js_sys::{Object, Reflect};
        use worker::wasm_bindgen::JsCast;
        let headers = Object::new();
        for (name, value) in [
            ("In-Reply-To", message_id.as_str()),
            ("References", message_id.as_str()),
            ("Auto-Submitted", "auto-replied"),
        ] {
            if Reflect::set(&headers, &name.into(), &value.into()) != Ok(true) {
                return failed(FailureCode::InvalidConfig);
            }
        }
        builder = builder.headers(&headers.unchecked_into());
    }
    let message = builder.build();
    let operation = Box::pin(binding.send_with_builder(&message));
    let deadline = Box::pin(Delay::from(Duration::from_millis(remaining)));
    match select(operation, deadline).await {
        Either::Left((Ok(result), _)) => Outcome::success(json!({
            "status":"accepted","messageId":result.message_id()
        })),
        Either::Left((Err(_), _)) => failed(FailureCode::External),
        Either::Right(_) => failed(FailureCode::OutcomeUnknown),
    }
}
