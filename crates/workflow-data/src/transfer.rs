//! Lossless public interchange. CSV stores a JSON value column, not inferred cell types.
use crate::*;
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportMode {
    #[default]
    Insert,
    Upsert,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportRow {
    pub key: String,
    pub value: Value,
    #[serde(default)]
    pub expires_at: Option<u64>,
}
pub fn preflight(
    store: &mut impl Store,
    table: &Table,
    rows: &[ImportRow],
    mode: &ImportMode,
    expected: Option<u64>,
    now: u64,
) -> Result<usize> {
    if rows.len() > 100 {
        return Err(error(
            ErrorCode::InvalidRequest,
            "Import supports up to 100 rows",
        ));
    }
    if serde_json::to_vec(rows).map_err(|_| invalid())?.len() > 65_536 {
        return Err(error(
            ErrorCode::InvalidRequest,
            "Import rows exceed 64 KiB",
        ));
    }
    if *mode == ImportMode::Upsert && expected.is_none() {
        return Err(error(
            ErrorCode::InvalidRequest,
            "Upsert import requires expectedDataRevision",
        ));
    }
    if expected.is_some_and(|v| v != table.data_revision) {
        return Err(error(
            ErrorCode::Conflict,
            "Data changed; preview the import again",
        ));
    }
    let mut keys = std::collections::BTreeSet::new();
    let mut skipped = 0;
    for row in rows {
        if !valid_key(&row.key)
            || !keys.insert(&row.key)
            || row.expires_at.is_some_and(|v| v > 9_007_199_254_740_991)
        {
            return Err(error(
                ErrorCode::InvalidRequest,
                "Invalid or duplicate import key or expiry",
            ));
        }
        validate_value(table, &row.value)?;
        if row.expires_at.is_some_and(|v| v <= now) {
            skipped += 1;
            continue;
        }
        if *mode == ImportMode::Insert
            && store
                .row(&table.definition.id, &row.key)?
                .is_some_and(|r| r.live(now))
        {
            return Err(error(
                ErrorCode::Conflict,
                "Import would overwrite an existing record",
            ));
        }
    }
    Ok(skipped)
}
fn invalid() -> DataError {
    error(
        ErrorCode::InvalidRequest,
        "Invalid transfer format, headers, rows or JSON values",
    )
}
pub fn parse(format: &str, content: &str) -> Result<Vec<ImportRow>> {
    if content.len() > 131_072 {
        return Err(error(
            ErrorCode::InvalidRequest,
            "Import file exceeds 128 KiB",
        ));
    }
    let rows = match format {
        "json" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Document {
                schema_version: u32,
                #[serde(default)]
                table_id: Option<String>,
                rows: Vec<ImportRow>,
            }
            let value: Value = serde_json::from_str(content).map_err(|_| invalid())?;
            if value.is_array() {
                serde_json::from_value(value).map_err(|_| invalid())?
            } else {
                let doc: Document = serde_json::from_value(value).map_err(|_| invalid())?;
                if doc.schema_version != 1 {
                    return Err(invalid());
                }
                let _ = doc.table_id;
                doc.rows
            }
        }
        "csv" => {
            let mut reader = csv::ReaderBuilder::new()
                .flexible(false)
                .from_reader(content.as_bytes());
            if reader
                .headers()
                .map_err(|_| invalid())?
                .iter()
                .collect::<Vec<_>>()
                != ["key", "value", "expiresAt"]
            {
                return Err(invalid());
            }
            let mut rows = Vec::new();
            for record in reader.records() {
                let record = record.map_err(|_| invalid())?;
                if rows.len() >= 100 {
                    return Err(invalid());
                }
                rows.push(ImportRow {
                    key: serde_json::from_str(record.get(0).ok_or_else(invalid)?)
                        .map_err(|_| invalid())?,
                    value: serde_json::from_str(record.get(1).ok_or_else(invalid)?)
                        .map_err(|_| invalid())?,
                    expires_at: if record.get(2) == Some("") {
                        None
                    } else {
                        Some(
                            record
                                .get(2)
                                .ok_or_else(invalid)?
                                .parse()
                                .map_err(|_| invalid())?,
                        )
                    },
                })
            }
            rows
        }
        _ => return Err(invalid()),
    };
    if rows.len() > 100 {
        return Err(error(
            ErrorCode::InvalidRequest,
            "Import supports up to 100 rows",
        ));
    }
    Ok(rows)
}
pub fn encode(format: &str, rows: &[ImportRow]) -> Result<String> {
    match format {
        "json" => serde_json::to_string(rows).map_err(|_| invalid()),
        "csv" => {
            let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
            writer
                .write_record(["key", "value", "expiresAt"])
                .map_err(|_| invalid())?;
            for row in rows {
                writer
                    .write_record([
                        serde_json::to_string(&row.key).map_err(|_| invalid())?,
                        row.value.to_string(),
                        row.expires_at.map(|v| v.to_string()).unwrap_or_default(),
                    ])
                    .map_err(|_| invalid())?
            }
            String::from_utf8(writer.into_inner().map_err(|_| invalid())?).map_err(|_| invalid())
        }
        _ => Err(invalid()),
    }
}
