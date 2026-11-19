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

fn configuration(node: &Node) -> Result<Value, FailureCode> {
    match node.config.get("command") {
        Some(Value::String(text)) => {
            serde_json::from_str(text).map_err(|_| FailureCode::InvalidConfig)
        }
        Some(value) => Ok(value.clone()),
        None => Err(FailureCode::InvalidConfig),
    }
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
    let configuration = configuration(node)?;
    let command = configuration
        .as_object()
        .ok_or(FailureCode::InvalidConfig)?;
    let operation = command
        .get("operation")
        .and_then(Value::as_str)
        .ok_or(FailureCode::InvalidConfig)?;
    let writes = match operation {
        "get_table" | "get" | "query" => false,
        "create_table" | "rename_table" | "insert_if_absent" | "put" | "patch" | "delete"
        | "increment" | "import_rows" => true,
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
    let mut value = configuration(node)?;
    let mut remaining = workflow_schema::MAX_CONFIG_BYTES;
    let fields = value.as_object_mut().ok_or(FailureCode::InvalidConfig)?;
    for key in [
        "key",
        "value",
        "patch",
        "removeFields",
        "expectedVersion",
        "expectedDataRevision",
        "rows",
        "amount",
        "initial",
        "ttlMs",
        "query",
    ] {
        if let Some(field) = fields.get_mut(key) {
            *field = resolve(field, input, 0, &mut remaining)?;
        }
    }
    let mut command_budget = workflow_schema::MAX_CONFIG_BYTES;
    charge(&value, &mut command_budget)?;
    let command: Command = serde_json::from_value(value).map_err(|_| FailureCode::InvalidConfig)?;
    // Defense against future command variants changing static routing semantics.
    if command.table_id() != access.table_id || command.mutates() != access.writes {
        return Err(FailureCode::InvalidConfig);
    }
    Ok(Prepared { command, access })
}

// Charge before cloning: many references to one input must not amplify memory
// beyond the node configuration budget. Serialization writes into a bounded sink.
fn charge(value: &Value, remaining: &mut usize) -> Result<(), FailureCode> {
    struct Budget<'a>(&'a mut usize);
    impl std::io::Write for Budget<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > *self.0 {
                return Err(std::io::Error::other("Configuration budget exceeded"));
            }
            *self.0 -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Budget(remaining), value).map_err(|_| FailureCode::BudgetExceeded)
}
fn resolve(
    value: &Value,
    input: &Value,
    depth: usize,
    remaining: &mut usize,
) -> Result<Value, FailureCode> {
    if depth > 64 {
        return Err(FailureCode::InvalidConfig);
    }
    match value {
        Value::Object(map) if map.contains_key("$input") => {
            if map.len() != 1 {
                return Err(FailureCode::InvalidConfig);
            }
            let pointer = map["$input"].as_str().ok_or(FailureCode::InvalidConfig)?;
            let resolved = input.pointer(pointer).ok_or(FailureCode::MissingInput)?;
            charge(resolved, remaining)?;
            Ok(resolved.clone())
        }
        Value::Object(map) => {
            let mut result = serde_json::Map::new();
            for (key, value) in map {
                charge(&Value::String(key.clone()), remaining)?;
                // Include separators and braces conservatively.
                *remaining = remaining
                    .checked_sub(3)
                    .ok_or(FailureCode::BudgetExceeded)?;
                result.insert(key.clone(), resolve(value, input, depth + 1, remaining)?);
            }
            Ok(Value::Object(result))
        }
        Value::Array(values) => {
            *remaining = remaining
                .checked_sub(values.len() + 2)
                .ok_or(FailureCode::BudgetExceeded)?;
            Ok(Value::Array(
                values
                    .iter()
                    .map(|value| resolve(value, input, depth + 1, remaining))
                    .collect::<Result<_, _>>()?,
            ))
        }
        value => {
            charge(value, remaining)?;
            Ok(value.clone())
        }
    }
}

/// This classification is shared by scheduling, hosting and permission checks.
pub fn handles(kind: &str) -> bool {
    matches!(kind, "Data Tables" | "Context Store")
}
pub fn manifests() -> Vec<connector_sdk::NodeManifest> {
    ["Data Tables", "Context Store"].into_iter().map(|kind| connector_sdk::NodeManifest {
        kind: kind.into(), version: 1,
        config_schema: serde_json::json!({"type":"object","properties":{"command":{"oneOf":[{"type":"object"},{"type":"string"}],"description":"Typed data command object or JSON-encoded object; static operation/tableId, explicit $input JSON-pointer references in payloads."}},"required":["command"],"additionalProperties":false}),
        output_ports: vec!["success".into(), "error".into()], has_external_side_effects: true,
    }).collect()
}

/// Execute within the host's rollback-capable transaction. The prepared kind is
/// checked against persisted metadata, including reads and existing create calls.
pub fn apply(
    store: &mut impl workflow_data::Store,
    prepared: Prepared,
    receipt: &str,
    now: u64,
) -> workflow_data::Result<Value> {
    if let Some(table) = store.table(&prepared.access.table_id)? {
        if table.definition.kind != prepared.access.kind {
            return Err(workflow_data::error(
                workflow_data::ErrorCode::InvalidRequest,
                "Dataset kind does not match connector",
            ));
        }
    }
    workflow_data::apply(store, prepared.command, Some(receipt), now)
}
