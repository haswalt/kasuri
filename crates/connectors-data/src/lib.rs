//! Pure configuration boundary for persistent data tasks. Hosts own transactions,
//! receipt lifetime and simulation scopes; this crate never selects a live store.
use connector_sdk::FailureCode;
use serde_json::Value;
use workflow_data::{Command, TableKind};
use workflow_schema::Node;

#[derive(Debug, Clone)]
pub struct Access {
    pub table_id: String,
    pub kind: TableKind,
    pub writes: bool,
}
#[derive(Debug, Clone)]
pub struct Prepared {
    pub command: Command,
    pub access: Access,
}

/// Inspect without input or data access. Dataset routing and permission needs may
/// not be supplied dynamically by a preceding node.
pub fn access(node: &Node) -> Result<Access, FailureCode> {
    let kind = match node.kind.as_str() {
        "Data Tables" => TableKind::Table,
        "Context Store" => TableKind::Context,
        _ => return Err(FailureCode::UnsupportedNode),
    };
    if node.version != 1 || node.config.len() != 1 {
        return Err(FailureCode::InvalidConfig);
    }
    let command = node
        .config
        .get("command")
        .and_then(Value::as_object)
        .ok_or(FailureCode::InvalidConfig)?;
    let operation = command
        .get("operation")
        .and_then(Value::as_str)
        .ok_or(FailureCode::InvalidConfig)?;
    let writes = match operation {
        "get_table" | "get" | "query" => false,
        "create_table" | "rename_table" | "insert_if_absent" | "put" | "patch" | "delete" => true,
        _ => return Err(FailureCode::InvalidConfig),
    };
    let id = if operation == "create_table" {
        let definition = command
            .get("definition")
            .and_then(Value::as_object)
            .ok_or(FailureCode::InvalidConfig)?;
        let configured: TableKind = serde_json::from_value(
            definition
                .get("kind")
                .cloned()
                .ok_or(FailureCode::InvalidConfig)?,
        )
        .map_err(|_| FailureCode::InvalidConfig)?;
        if configured != kind {
            return Err(FailureCode::InvalidConfig);
        }
        definition.get("id")
    } else {
        command.get("tableId")
    }
    .and_then(Value::as_str)
    .ok_or(FailureCode::InvalidConfig)?;
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(FailureCode::InvalidConfig);
    }
    Ok(Access {
        table_id: id.into(),
        kind,
        writes,
    })
}

/// Resolve explicit JSON-pointer input references only in record/query payloads.
/// Static dataset metadata is retained literally. No expression evaluation,
/// environment access, arbitrary SQL or caller-selected receipt namespace.
pub fn prepare(node: &Node, input: &Value) -> Result<Prepared, FailureCode> {
    let access = access(node)?;
    let mut value = node.config["command"].clone();
    let fields = value.as_object_mut().ok_or(FailureCode::InvalidConfig)?;
    for key in [
        "key",
        "value",
        "patch",
        "removeFields",
        "expectedVersion",
        "ttlMs",
        "query",
    ] {
        if let Some(field) = fields.get_mut(key) {
            *field = resolve(field, input, 0)?;
        }
    }
    let command: Command = serde_json::from_value(value).map_err(|_| FailureCode::InvalidConfig)?;
    // Defense against future command variants changing static routing semantics.
    if command.table_id() != access.table_id || command.mutates() != access.writes {
        return Err(FailureCode::InvalidConfig);
    }
    Ok(Prepared { command, access })
}

fn resolve(value: &Value, input: &Value, depth: usize) -> Result<Value, FailureCode> {
    if depth > 64 {
        return Err(FailureCode::InvalidConfig);
    }
    match value {
        Value::Object(map) if map.contains_key("$input") => {
            if map.len() != 1 {
                return Err(FailureCode::InvalidConfig);
            }
            let pointer = map["$input"].as_str().ok_or(FailureCode::InvalidConfig)?;
            input
                .pointer(pointer)
                .cloned()
                .ok_or(FailureCode::MissingInput)
        }
        Value::Object(map) => Ok(Value::Object(
            map.iter()
                .map(|(key, value)| Ok((key.clone(), resolve(value, input, depth + 1)?)))
                .collect::<Result<_, FailureCode>>()?,
        )),
        Value::Array(values) => Ok(Value::Array(
            values
                .iter()
                .map(|value| resolve(value, input, depth + 1))
                .collect::<Result<_, _>>()?,
        )),
        value => Ok(value.clone()),
    }
}
