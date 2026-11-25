//! Per-task outbound MCP sessions. Operators scope destinations, tools and credentials.
use crate::{api_error, now};
use connector_sdk::{FailureCode, Outcome, Task};
use connectors_http::{MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, redact};
use connectors_mcp_client as protocol;
use futures_util::{
    StreamExt,
    future::{Either, select},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet, HashSet},
    time::Duration,
};
use worker::*;

type ClientResult<T> = std::result::Result<T, FailureCode>;

// Keep a full discovery below the Worker subrequest budget even with multiple pages.
const MAX_LIST_PAGES: u32 = 16;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Profile {
    url: String,
    workspace_ids: BTreeSet<String>,
    allowed_tools: BTreeSet<String>,
    bearer_token: Option<String>,
}

impl Profile {
    fn url(&self) -> ClientResult<Url> {
        let url = Url::parse(&self.url).map_err(|_| FailureCode::InvalidConfig)?;
        if url.scheme() != "https"
            || url.host().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(FailureCode::InvalidConfig);
        }
        if self.bearer_token.as_ref().is_some_and(|value| {
            value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_graphic())
        }) {
            return Err(FailureCode::InvalidConfig);
        }
        Ok(url)
    }
}

fn profiles(env: &Env) -> ClientResult<BTreeMap<String, Profile>> {
    let setting = env
        .secret("MCP_CONNECTIONS")
        .or_else(|_| env.var("MCP_CONNECTIONS"))
        .map(|value| value.to_string())
        .unwrap_or_default();
    if setting.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    serde_json::from_str(&setting).map_err(|_| FailureCode::InvalidConfig)
}

pub(crate) fn list_connections(env: &Env, workspace: &str) -> Result<Response> {
    let Ok(profiles) = profiles(env) else {
        return api_error(500, "Invalid MCP connection configuration");
    };
    let connections: Vec<_> = profiles
        .into_iter()
        .filter(|(alias, profile)| {
            protocol::valid_alias(alias) && profile.workspace_ids.contains(workspace)
        })
        .map(|(alias, profile)| json!({"alias":alias,"tools":profile.allowed_tools}))
        .collect();
    Response::from_json(&json!({"connections":connections}))
}

struct Session<'a> {
    url: Url,
    token: Option<&'a str>,
    version: Option<String>,
    id: Option<String>,
    signal: &'a AbortSignal,
}
impl Session<'_> {
    async fn send(&self, method: Method, payload: Option<Value>) -> ClientResult<Response> {
        let headers = Headers::new();
        headers
            .set("Accept", "application/json, text/event-stream")
            .map_err(|_| FailureCode::InvalidConfig)?;
        if let Some(token) = self.token {
            headers
                .set("Authorization", &format!("Bearer {token}"))
                .map_err(|_| FailureCode::InvalidConfig)?;
        }
        if let Some(version) = &self.version {
            headers
                .set("MCP-Protocol-Version", version)
                .map_err(|_| FailureCode::InvalidConfig)?;
        }
        if let Some(id) = &self.id {
            headers
                .set("Mcp-Session-Id", id)
                .map_err(|_| FailureCode::InvalidConfig)?;
        }
        let mut init = RequestInit::new();
        init.with_method(method)
            .with_headers(headers.clone())
            .with_redirect(RequestRedirect::Manual);
        if let Some(payload) = payload {
            let text = payload.to_string();
            if text.len() > MAX_REQUEST_BYTES {
                return Err(FailureCode::BudgetExceeded);
            }
            headers
                .set("Content-Type", "application/json")
                .map_err(|_| FailureCode::InvalidConfig)?;
            init.with_headers(headers).with_body(Some(text.into()));
        }
        let request = Request::new_with_init(self.url.as_str(), &init)
            .map_err(|_| FailureCode::InvalidConfig)?;
        let response = Fetch::Request(request)
            .send_with_signal(self.signal)
            .await
            .map_err(|_| FailureCode::External)?;
        if (300..400).contains(&response.status_code()) {
            return Err(FailureCode::EgressDenied);
        }
        Ok(response)
    }

    async fn connect(&mut self) -> ClientResult<()> {
        let mut response = self
            .send(Method::Post, Some(protocol::initialize()))
            .await?;
        let result = read_rpc(&mut response, 1).await?;
        self.version = Some(protocol::negotiate(&result)?.to_owned());
        self.id = response
            .headers()
            .get("Mcp-Session-Id")
            .map_err(|_| FailureCode::InvalidResponse)?;
        if self.id.as_ref().is_some_and(|id| {
            id.is_empty() || id.len() > 1024 || !id.bytes().all(|byte| byte.is_ascii_graphic())
        }) {
            self.id = None;
            return Err(FailureCode::InvalidResponse);
        }
        let response = self
            .send(
                Method::Post,
                Some(json!({
                    "jsonrpc":"2.0","method":"notifications/initialized"
                })),
            )
            .await?;
        if response.status_code() != 202 {
            return Err(FailureCode::External);
        }
        Ok(())
    }

    async fn request(&self, id: u32, method: &str, params: Value) -> ClientResult<Value> {
        let mut response = self
            .send(Method::Post, Some(protocol::rpc(id, method, params)))
            .await?;
        read_rpc(&mut response, id).await
    }

    async fn discover(&self, allowed: &BTreeSet<String>) -> ClientResult<Value> {
        let mut tools = BTreeMap::new();
        let mut cursors = HashSet::new();
        let mut cursor: Option<String> = None;
        let mut output_bytes = 0;
        for id in 2..2 + MAX_LIST_PAGES {
            let params = cursor
                .as_ref()
                .map_or_else(|| json!({}), |value| json!({"cursor":value}));
            let result = self.request(id, "tools/list", params).await?;
            let page = protocol::list_page(&result)?;
            for tool in page.tools {
                let name = tool["name"].as_str().unwrap();
                if allowed.contains(name) && !tools.contains_key(name) {
                    let contract = protocol::tool_contract(tool);
                    output_bytes += contract.to_string().len();
                    if output_bytes > protocol::MAX_OUTPUT_BYTES {
                        return Err(FailureCode::BudgetExceeded);
                    }
                    tools.insert(name.to_owned(), contract);
                }
            }
            if tools.len() == allowed.len() || page.cursor.is_none() {
                return Ok(
                    json!({"tools":tools.into_values().collect::<Vec<_>>(),"truncated":false}),
                );
            }
            cursor = page.cursor.map(str::to_owned);
            if !cursors.insert(cursor.clone()) {
                return Err(FailureCode::InvalidResponse);
            }
        }
        Ok(json!({"tools":tools.into_values().collect::<Vec<_>>(),"truncated":true}))
    }

    async fn close(&self, deadline: u64) {
        if self.id.is_some() && deadline.saturating_sub(now()) > 250 {
            let request = Box::pin(self.send(Method::Delete, None));
            let grace = Box::pin(Delay::from(Duration::from_millis(150)));
            let _ = select(request, grace).await;
        }
    }
}

async fn read_rpc(response: &mut Response, id: u32) -> ClientResult<Value> {
    if response.status_code() != 200 {
        return Err(FailureCode::External);
    }
    let content_type = response
        .headers()
        .get("Content-Type")
        .map_err(|_| FailureCode::InvalidResponse)?
        .unwrap_or_default();
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let sse = match media_type.as_str() {
        "text/event-stream" => true,
        "application/json" => false,
        _ => return Err(FailureCode::InvalidResponse),
    };
    let mut bytes = Vec::new();
    let mut total: usize = 0;
    let mut decoder = Sse::default();
    let mut stream = response
        .stream()
        .map_err(|_| FailureCode::InvalidResponse)?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| FailureCode::External)?;
        total = total.saturating_add(chunk.len());
        if total > MAX_RESPONSE_BYTES {
            return Err(FailureCode::BudgetExceeded);
        }
        if sse {
            for byte in chunk {
                if let Some(event) = decoder.push(byte)? {
                    if let Some(value) = protocol::rpc_result(event, id)? {
                        return Ok(value);
                    }
                }
            }
        } else {
            bytes.extend(chunk);
        }
    }
    if sse {
        return Err(FailureCode::InvalidResponse);
    }
    let document = serde_json::from_slice(&bytes).map_err(|_| FailureCode::InvalidResponse)?;
    protocol::rpc_result(document, id)?.ok_or(FailureCode::InvalidResponse)
}

#[derive(Default)]
struct Sse {
    line: Vec<u8>,
    data: Vec<u8>,
    cr: bool,
    saw_line: bool,
}
impl Sse {
    fn push(&mut self, byte: u8) -> ClientResult<Option<Value>> {
        let cr = std::mem::replace(&mut self.cr, byte == b'\r');
        if cr && byte == b'\n' {
            return Ok(None);
        }
        if !matches!(byte, b'\r' | b'\n') {
            self.line.push(byte);
            return Ok(None);
        }
        if self.line.is_empty() && !self.data.is_empty() {
            let data = std::mem::take(&mut self.data);
            // Priming events and keepalives can have an empty data field.
            if data.iter().all(u8::is_ascii_whitespace) {
                return Ok(None);
            }
            return serde_json::from_slice(&data)
                .map(Some)
                .map_err(|_| FailureCode::InvalidResponse);
        }
        let line = if std::mem::replace(&mut self.saw_line, true) {
            &self.line
        } else {
            self.line
                .strip_prefix(b"\xef\xbb\xbf")
                .unwrap_or(&self.line)
        };
        if let Some(data) = line.strip_prefix(b"data:") {
            self.data.extend(data.strip_prefix(b" ").unwrap_or(data));
            self.data.push(b'\n');
        }
        self.line.clear();
        Ok(None)
    }
}

pub(crate) async fn execute(task: &Task, env: &Env, workspace: &str) -> Outcome {
    let fail = protocol::failed;
    let config = match protocol::config(&task.node.config) {
        Ok(config) => config,
        Err(code) => return fail(code),
    };
    let mut profiles = match profiles(env) {
        Ok(profiles) => profiles,
        Err(code) => return fail(code),
    };
    let Some(profile) = profiles.remove(&config.connection) else {
        return fail(FailureCode::MissingInput);
    };
    if !profile.workspace_ids.contains(workspace)
        || config
            .tool
            .as_ref()
            .is_some_and(|tool| !profile.allowed_tools.contains(tool))
    {
        return fail(FailureCode::EgressDenied);
    }
    if profile.allowed_tools.is_empty()
        || !profile
            .allowed_tools
            .iter()
            .all(|tool| protocol::valid_tool(tool))
    {
        return fail(FailureCode::InvalidConfig);
    }
    let url = match profile.url() {
        Ok(url) => url,
        Err(code) => return fail(code),
    };
    let arguments = if config.operation == protocol::Operation::Call {
        match config.arguments(&task.input) {
            Ok(arguments) => Some(arguments),
            Err(code) => return fail(code),
        }
    } else {
        None
    };
    let remaining = task.deadline_ms.saturating_sub(now());
    if remaining == 0 {
        return fail(FailureCode::Timeout);
    }
    let controller = AbortController::default();
    let signal = controller.signal();
    let call_dispatched = Cell::new(false);
    let run = Box::pin(async {
        let variable = |key| {
            env.var(key)
                .map(|value| value.to_string())
                .unwrap_or_default()
        };
        if !connectors_http::allowed_url(&url, &variable("HTTP_ALLOWED_HOSTS"))
            || (!connectors_http::private_url_opt_in(&url, &variable("HTTP_PRIVATE_HOSTS"))
                && (!connectors_http::public_url_destination(&url)
                    || !crate::http::public_dns(&url, &signal)
                        .await
                        .unwrap_or(false)))
        {
            return Err(FailureCode::EgressDenied);
        }
        let mut session = Session {
            url,
            token: profile.bearer_token.as_deref(),
            version: None,
            id: None,
            signal: &signal,
        };
        let result = async {
            session.connect().await?;
            if let Some(arguments) = arguments {
                let tool = config.tool.as_deref().unwrap();
                call_dispatched.set(true);
                let result = session
                    .request(2, "tools/call", json!({"name":tool,"arguments":arguments}))
                    .await?;
                protocol::call_result(tool, result)
            } else {
                session.discover(&profile.allowed_tools).await
            }
        }
        .await;
        session.close(task.deadline_ms).await;
        let mut result = result?;
        let mut secrets: Vec<_> = profile.bearer_token.iter().cloned().collect();
        secrets.extend(session.id);
        redact(&mut result, &secrets);
        protocol::bounded_output(result)
    });
    let timeout = Box::pin(Delay::from(Duration::from_millis(remaining)));
    let outcome = match select(run, timeout).await {
        Either::Left((result, _)) => result.map_or_else(fail, Outcome::success),
        Either::Right(_) if call_dispatched.get() => fail(FailureCode::OutcomeUnknown),
        Either::Right(_) => fail(FailureCode::Timeout),
    };
    controller.abort();
    outcome
}
