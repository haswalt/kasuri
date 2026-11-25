//! Configuration, JSON-RPC envelopes and offline results for outbound MCP calls.
use connector_sdk::{Failure, FailureCode, NodeManifest, Outcome, Task};
use serde::Deserialize;
use serde_json::{Map, Value, json};

pub const KIND: &str = "MCP Client";
pub const PROTOCOL: &str = "2025-11-25";
// The existing engine budget for a single node value.
pub const MAX_OUTPUT_BYTES: usize = 65_536;

#[derive(Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    #[default]
    Call,
    List,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub connection: String,
    #[serde(default)]
    pub operation: Operation,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    arguments_pointer: Option<String>,
    #[serde(default)]
    simulation_response: Option<Value>,
}

pub fn valid_alias(alias: &str) -> bool {
    !alias.is_empty()
        && alias
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
}

pub fn valid_tool(tool: &str) -> bool {
    !tool.is_empty()
        && tool.len() <= 128
        && tool
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
}

pub fn config(input: &Map<String, Value>) -> Result<Config, FailureCode> {
    let config: Config = serde_json::from_value(Value::Object(input.clone()))
        .map_err(|_| FailureCode::InvalidConfig)?;
    let valid_operation = match config.operation {
        Operation::Call => config.tool.as_deref().is_some_and(valid_tool),
        Operation::List => config.tool.is_none() && config.arguments_pointer.is_none(),
    };
    let valid_pointer = config.arguments_pointer.as_ref().is_none_or(|pointer| {
        pointer.is_empty()
            || (pointer.starts_with('/')
                && pointer
                    .split('~')
                    .skip(1)
                    .all(|s| s.starts_with(['0', '1'])))
    });
    if !valid_alias(&config.connection) || !valid_operation || !valid_pointer {
        return Err(FailureCode::InvalidConfig);
    }
    if let Some(value) = &config.simulation_response {
        match config.operation {
            Operation::Call => {
                if !value.is_object() {
                    return Err(FailureCode::InvalidConfig);
                }
                if !matches!(
                    call_result(config.tool.as_deref().unwrap(), value.clone()),
                    Ok(_) | Err(FailureCode::External)
                ) {
                    return Err(FailureCode::InvalidConfig);
                }
            }
            Operation::List => {
                list_page(value).map_err(|_| FailureCode::InvalidConfig)?;
            }
        }
    }
    Ok(config)
}

impl Config {
    pub fn arguments<'a>(&self, input: &'a Value) -> Result<&'a Map<String, Value>, FailureCode> {
        input
            .pointer(self.arguments_pointer.as_deref().unwrap_or(""))
            .and_then(Value::as_object)
            .ok_or(FailureCode::MissingInput)
    }
}

pub fn rpc(id: u32, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}
pub fn initialize() -> Value {
    rpc(
        1,
        "initialize",
        json!({
            "protocolVersion": PROTOCOL,
            "capabilities": {},
            "clientInfo": {"name":"Kasuri","version":env!("CARGO_PKG_VERSION")}
        }),
    )
}
pub fn negotiate(result: &Value) -> Result<&str, FailureCode> {
    if !result["capabilities"]["tools"].is_object() {
        return Err(FailureCode::InvalidResponse);
    }
    result["protocolVersion"]
        .as_str()
        .filter(|version| [PROTOCOL, "2025-06-18", "2025-03-26"].contains(version))
        .ok_or(FailureCode::InvalidResponse)
}

pub fn rpc_result(mut message: Value, id: u32) -> Result<Option<Value>, FailureCode> {
    if message["jsonrpc"] != "2.0" {
        return Err(FailureCode::InvalidResponse);
    }
    if message.get("method").is_some() {
        // This client negotiates no server-initiated features.
        return if message.get("id").is_none() {
            Ok(None)
        } else {
            Err(FailureCode::InvalidResponse)
        };
    }
    if message["id"].as_u64() != Some(u64::from(id)) {
        return Err(FailureCode::InvalidResponse);
    }
    let has_error = message.get("error").is_some();
    let valid_error = message["error"]["code"].is_i64() && message["error"]["message"].is_string();
    match (
        message
            .as_object_mut()
            .and_then(|object| object.remove("result")),
        has_error,
    ) {
        (Some(result), false) if result.is_object() => Ok(Some(result)),
        (None, true) if valid_error => Err(FailureCode::External),
        _ => Err(FailureCode::InvalidResponse),
    }
}

pub fn call_result(tool: &str, result: Value) -> Result<Value, FailureCode> {
    match result.get("isError") {
        Some(Value::Bool(true)) => return Err(FailureCode::External),
        None | Some(Value::Bool(false)) => {}
        _ => return Err(FailureCode::InvalidResponse),
    }
    let content = result["content"]
        .as_array()
        .ok_or(FailureCode::InvalidResponse)?;
    let structured = &result["structuredContent"];
    if (!structured.is_null() && !structured.is_object())
        || content.iter().any(|item| !item["type"].is_string())
    {
        return Err(FailureCode::InvalidResponse);
    }
    Ok(json!({"tool":tool,"content":content,"structuredContent":structured}))
}

pub struct ToolPage<'a> {
    pub tools: &'a [Value],
    pub cursor: Option<&'a str>,
}

pub fn list_page(result: &Value) -> Result<ToolPage<'_>, FailureCode> {
    let tools = result["tools"]
        .as_array()
        .ok_or(FailureCode::InvalidResponse)?;
    if tools
        .iter()
        .any(|tool| !tool["name"].is_string() || !tool["inputSchema"].is_object())
    {
        return Err(FailureCode::InvalidResponse);
    }
    let cursor = match result.get("nextCursor") {
        None | Some(Value::Null) => None,
        Some(Value::String(cursor)) if !cursor.is_empty() => Some(cursor.as_str()),
        _ => return Err(FailureCode::InvalidResponse),
    };
    Ok(ToolPage { tools, cursor })
}

pub fn tool_contract(tool: &Value) -> Value {
    json!({
        "name": tool["name"],
        "description":tool["description"].as_str(),
        "inputSchema":tool["inputSchema"],
        "outputSchema":tool.get("outputSchema").filter(|value| value.is_object()),
    })
}

pub fn simulate(task: &Task) -> Outcome {
    let result = (|| {
        let config = config(&task.node.config)?;
        let result = config
            .simulation_response
            .as_ref()
            .ok_or(FailureCode::InvalidConfig)?;
        match config.operation {
            Operation::Call => {
                config.arguments(&task.input)?;
                call_result(config.tool.as_deref().unwrap(), result.clone())
            }
            Operation::List => {
                let page = list_page(result)?;
                Ok(
                    json!({"tools":page.tools.iter().map(tool_contract).collect::<Vec<_>>(),"truncated":page.cursor.is_some()}),
                )
            }
        }
    })();
    result
        .and_then(bounded_output)
        .map_or_else(failed, Outcome::success)
}

pub fn bounded_output(value: Value) -> Result<Value, FailureCode> {
    if value.to_string().len() > MAX_OUTPUT_BYTES {
        return Err(FailureCode::BudgetExceeded);
    }
    Ok(value)
}

pub fn failed(code: FailureCode) -> Outcome {
    Outcome::Failed {
        failure: Failure::permanent(code),
    }
}

pub fn output_schema(list: bool) -> Value {
    if list {
        json!({"type":"object","properties":{"tools":{"type":"array","items":{"type":"object","properties":{
            "name":{"type":"string"},"description":{"type":["string","null"]},"inputSchema":{"type":"object"},"outputSchema":{"type":["object","null"]}
        }}},"truncated":{"type":"boolean"}},"required":["tools","truncated"]})
    } else {
        json!({"type":"object","properties":{"tool":{"type":"string"},"content":{"type":"array"},"structuredContent":{"type":["object","null"]}},"required":["tool","content","structuredContent"]})
    }
}

pub fn manifest() -> NodeManifest {
    NodeManifest {
        kind: KIND.into(),
        version: 1,
        config_schema: json!({
            "type":"object","additionalProperties":false,
            "required":["connection"],"properties":{
                "connection":{"type":"string","pattern":"^[A-Za-z0-9_-]+$"},
                "operation":{"enum":["call","list"],"default":"call"},
                "tool":{"type":"string","pattern":"^[A-Za-z0-9_.-]{1,128}$"},
                "argumentsPointer":{"type":"string","description":"JSON Pointer to a tool arguments object. Empty selects the whole input."},
                "simulationResponse":{"type":"object","description":"Synthetic MCP result object used only in simulation."}
            },
            "if":{"required":["operation"],"properties":{"operation":{"const":"list"}}},
            "then":{"not":{"anyOf":[{"required":["tool"]},{"required":["argumentsPointer"]}]}},
            "else":{"required":["tool"]}
        }),
        output_ports: vec!["success".into(), "error".into()],
        has_external_side_effects: true,
    }
}
