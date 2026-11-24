//! Compact checkpoint encoding: large values that appear more than once in a run's
//! state are stored once and referenced.
//!
//! Data passes through workflows unchanged a lot: a trigger's output is the run's
//! input, a Wait node outputs what it received, routing nodes forward items. Each
//! copy used to be stored again in the checkpoint, so a run's stored state was a
//! multiple of its data. A compact checkpoint is
//!
//! ```json
//! {"compactCheckpoint": 1, "values": [...], "run": {...}}
//! ```
//!
//! where every repeated value of at least [`SHARED_BYTES`] in `run` is replaced by
//! `{"$ref": index}` into `values`. An original object whose only key is `$ref` or
//! `$esc` is wrapped as `{"$esc": object}`, so data can never be mistaken for a
//! reference. Plain checkpoints (anything without `compactCheckpoint`) are read
//! as before.
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Values smaller than this are cheaper to repeat than to reference.
pub const SHARED_BYTES: usize = 1024;
const MARKER: &str = "compactCheckpoint";
const VERSION: u64 = 1;
const REF: &str = "$ref";
const ESCAPE: &str = "$esc";

fn encoded(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
fn needs_escape(map: &Map<String, Value>) -> bool {
    map.len() == 1 && (map.contains_key(REF) || map.contains_key(ESCAPE))
}

/// A subtree's structural hash and (approximate) encoded size, computed bottom-up
/// once, so finding repeats is linear in the size of the state.
struct Shape {
    hash: u64,
    len: usize,
    children: Vec<Shape>,
}
fn shape(value: &Value) -> Shape {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    let (len, children) = match value {
        Value::Object(map) => {
            0u8.hash(&mut hasher);
            let children: Vec<Shape> = map.values().map(shape).collect();
            let mut len = 2 + map.len().saturating_sub(1);
            for ((key, _), child) in map.iter().zip(&children) {
                key.hash(&mut hasher);
                child.hash.hash(&mut hasher);
                len += key.len() + 3 + child.len;
            }
            (len, children)
        }
        Value::Array(items) => {
            1u8.hash(&mut hasher);
            let children: Vec<Shape> = items.iter().map(shape).collect();
            let mut len = 2 + items.len().saturating_sub(1);
            for child in &children {
                child.hash.hash(&mut hasher);
                len += child.len;
            }
            (len, children)
        }
        Value::String(text) => {
            2u8.hash(&mut hasher);
            text.hash(&mut hasher);
            (text.len() + 2, Vec::new())
        }
        other => {
            3u8.hash(&mut hasher);
            let text = other.to_string();
            text.hash(&mut hasher);
            (text.len(), Vec::new())
        }
    };
    Shape {
        hash: hasher.finish(),
        len,
        children,
    }
}
fn shareable(value: &Value, shape: &Shape) -> bool {
    shape.len >= SHARED_BYTES
        && matches!(value, Value::Object(_) | Value::Array(_) | Value::String(_))
}
/// Count large subtrees by hash.
fn count(value: &Value, shape: &Shape, counts: &mut HashMap<u64, usize>) {
    match value {
        Value::Object(map) => {
            for (child, child_shape) in map.values().zip(&shape.children) {
                count(child, child_shape, counts);
            }
        }
        Value::Array(items) => {
            for (child, child_shape) in items.iter().zip(&shape.children) {
                count(child, child_shape, counts);
            }
        }
        _ => {}
    }
    if shareable(value, shape) {
        *counts.entry(shape.hash).or_default() += 1;
    }
}

struct Sharing<'a> {
    counts: &'a HashMap<u64, usize>,
    table: Vec<Value>,
    /// Entries by hash, with the original value to confirm equality: a hash
    /// collision is never shared.
    index: HashMap<u64, Vec<(usize, Value)>>,
}
impl Sharing<'_> {
    /// Replace repeated large subtrees with references. A shared value goes into
    /// the table once, with its own repeated parts shared in turn.
    fn share(&mut self, value: Value, shape: &Shape) -> Value {
        if shareable(&value, shape) && self.counts.get(&shape.hash).is_some_and(|n| *n > 1) {
            let existing = self.index.get(&shape.hash).and_then(|entries| {
                entries
                    .iter()
                    .find(|(_, original)| *original == value)
                    .map(|(at, _)| *at)
            });
            if let Some(at) = existing {
                return reference(at);
            }
            let at = self.table.len();
            self.index
                .entry(shape.hash)
                .or_default()
                .push((at, value.clone()));
            self.table.push(Value::Null);
            let entry = self.share_children(value, shape);
            self.table[at] = entry;
            return reference(at);
        }
        self.share_children(value, shape)
    }
    fn share_children(&mut self, value: Value, shape: &Shape) -> Value {
        match value {
            Value::Object(map) => {
                let escaped = needs_escape(&map);
                let map: Map<String, Value> = map
                    .into_iter()
                    .zip(&shape.children)
                    .map(|((k, v), child)| (k, self.share(v, child)))
                    .collect();
                if escaped {
                    Value::Object(Map::from_iter([(ESCAPE.into(), Value::Object(map))]))
                } else {
                    Value::Object(map)
                }
            }
            Value::Array(items) => Value::Array(
                items
                    .into_iter()
                    .zip(&shape.children)
                    .map(|(v, child)| self.share(v, child))
                    .collect(),
            ),
            other => other,
        }
    }
}
fn reference(at: usize) -> Value {
    Value::Object(Map::from_iter([(REF.into(), Value::from(at))]))
}

/// Encode run state compactly. Returns the plain encoding when nothing repeats.
pub fn encode(state: Value) -> String {
    let shape = shape(&state);
    let mut counts = HashMap::new();
    count(&state, &shape, &mut counts);
    if !counts.values().any(|n| *n > 1) {
        return encoded(&state);
    }
    let mut sharing = Sharing {
        counts: &counts,
        table: Vec::new(),
        index: HashMap::new(),
    };
    let run = sharing.share(state, &shape);
    encoded(&Value::Object(Map::from_iter([
        (MARKER.into(), Value::from(VERSION)),
        ("values".into(), Value::Array(sharing.table)),
        ("run".into(), run),
    ])))
}

/// Whether checkpoint text is in the compact form, without parsing it. Compact
/// checkpoints are always written with the marker as their first key.
pub fn is_compact_text(text: &str) -> bool {
    text.trim_start()
        .strip_prefix('{')
        .is_some_and(|rest| rest.trim_start().starts_with("\"compactCheckpoint\""))
}
/// Whether a parsed checkpoint is in the compact form.
fn is_compact(value: &Value) -> bool {
    value.get(MARKER).is_some()
}

/// Expansion state: which entries are being expanded (to reject cycles) and how
/// many bytes references may still add (to reject inputs that would balloon).
struct Expansion<'a> {
    table: &'a [Value],
    active: Vec<bool>,
    budget: usize,
}
impl Expansion<'_> {
    fn expand(&mut self, value: Value) -> Result<Value, String> {
        match value {
            Value::Object(mut map) if map.len() == 1 && map.contains_key(REF) => {
                let index = map
                    .remove(REF)
                    .and_then(|v| v.as_u64())
                    .ok_or("invalid checkpoint reference")? as usize;
                let shared = self
                    .table
                    .get(index)
                    .ok_or("checkpoint reference out of range")?
                    .clone();
                if std::mem::replace(&mut self.active[index], true) {
                    return Err("checkpoint references form a cycle".into());
                }
                let expanded = self.expand(shared)?;
                self.active[index] = false;
                self.budget = self
                    .budget
                    .checked_sub(encoded(&expanded).len())
                    .ok_or("checkpoint expands beyond its size limit")?;
                Ok(expanded)
            }
            Value::Object(mut map) if map.len() == 1 && map.contains_key(ESCAPE) => {
                match map.remove(ESCAPE) {
                    Some(Value::Object(inner)) => self.expand_map(inner),
                    _ => Err("invalid checkpoint escape".into()),
                }
            }
            Value::Object(map) => self.expand_map(map),
            Value::Array(items) => Ok(Value::Array(
                items
                    .into_iter()
                    .map(|v| self.expand(v))
                    .collect::<Result<_, _>>()?,
            )),
            other => Ok(other),
        }
    }
    fn expand_map(&mut self, map: Map<String, Value>) -> Result<Value, String> {
        Ok(Value::Object(
            map.into_iter()
                .map(|(k, v)| Ok((k, self.expand(v)?)))
                .collect::<Result<_, String>>()?,
        ))
    }
}

/// Parse a checkpoint, compact or plain, into the full run state. References
/// may add at most `limit` bytes in total.
pub fn decode(text: &str, limit: usize) -> Result<Value, String> {
    let value: Value = serde_json::from_str(text).map_err(|_| "invalid checkpoint JSON")?;
    if !is_compact(&value) {
        return Ok(value);
    }
    let Value::Object(mut root) = value else {
        return Err("invalid compact checkpoint".into());
    };
    if root.get(MARKER).and_then(Value::as_u64) != Some(VERSION) {
        return Err("unsupported compact checkpoint version".into());
    }
    let table = match root.remove("values") {
        Some(Value::Array(table)) => table,
        _ => return Err("compact checkpoint has no value table".into()),
    };
    let run = root.remove("run").ok_or("compact checkpoint has no run")?;
    Expansion {
        active: vec![false; table.len()],
        table: &table,
        budget: limit,
    }
    .expand(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn roundtrip(value: Value) -> String {
        let text = encode(value.clone());
        assert_eq!(decode(&text, usize::MAX).unwrap(), value, "{text}");
        text
    }

    #[test]
    fn repeated_large_values_are_stored_once() {
        let big = "x".repeat(5_000);
        let state = json!({"input":{"payload":big},"nodes":{"a":{"output":{"payload":big}},"b":{"output":{"payload":big}}}});
        let text = roundtrip(state.clone());
        assert_eq!(text.matches(&big).count(), 1);
        assert!(text.len() < encoded(&state).len() / 2);
    }

    #[test]
    fn small_or_unique_values_keep_the_plain_encoding() {
        let state = json!({"input":{"a":1},"nodes":{"x":{"output":{"a":1}},"y":{"output":"y".repeat(5_000)}}});
        assert_eq!(roundtrip(state.clone()), encoded(&state));
    }

    #[test]
    fn data_that_looks_like_a_reference_is_preserved() {
        let big = "z".repeat(2_000);
        for tricky in [
            json!({"$ref": 0}),
            json!({"$esc": {"$ref": 0}}),
            json!({"$esc": 1}),
            json!({"$ref": "not a number"}),
            json!([{"$ref": 0}, {"$esc": {}}]),
        ] {
            // Alone, and inside repeated (shared) values.
            roundtrip(json!({"input": tricky}));
            let shared = json!({"tricky": tricky, "big": big});
            roundtrip(json!({"input": shared, "nodes": {"a": {"output": shared}}}));
        }
    }

    #[test]
    fn compact_text_is_recognised_without_parsing() {
        let big = "x".repeat(5_000);
        let compact = encode(json!({"a": big, "b": big}));
        assert!(is_compact_text(&compact), "{}", &compact[..40]);
        assert!(!is_compact_text(&encode(json!({"a": 1}))));
        assert!(!is_compact_text(
            r#"{"checkpointVersion":1,"compactCheckpoint":1}"#
        ));
    }

    #[test]
    fn expansion_is_bounded() {
        // Each entry doubles the previous one: tiny when compact, huge expanded.
        let mut values = vec![json!("b".repeat(1_000))];
        for i in 0..30 {
            values.push(json!([{"$ref": i}, {"$ref": i}]));
        }
        let bomb = json!({"compactCheckpoint": 1, "values": values, "run": {"$ref": 30}});
        assert!(decode(&bomb.to_string(), 2_097_152).is_err());
    }

    #[test]
    fn nested_repeats_share_the_outermost_value() {
        let inner = json!({"blob": "q".repeat(3_000)});
        let outer = json!({"inner": inner, "more": "m".repeat(1_500)});
        let state = json!({"a": outer, "b": outer, "c": inner});
        let text = roundtrip(state);
        assert_eq!(
            text.matches(&"q".repeat(3_000)).count(),
            1,
            "{}",
            text.len()
        );
    }

    #[test]
    fn malformed_compact_checkpoints_are_rejected() {
        for bad in [
            json!({"compactCheckpoint": 2, "values": [], "run": {}}),
            json!({"compactCheckpoint": 1, "run": {}}),
            json!({"compactCheckpoint": 1, "values": []}),
            json!({"compactCheckpoint": 1, "values": [], "run": {"x": {"$ref": 3}}}),
            json!({"compactCheckpoint": 1, "values": [], "run": {"x": {"$esc": 1}}}),
            // Cycles are rejected, not followed.
            json!({"compactCheckpoint": 1, "values": [{"$ref": 0}], "run": {"x": {"$ref": 0}}}),
            json!({"compactCheckpoint": 1, "values": [{"a": {"$ref": 1}}, {"b": {"$ref": 0}}], "run": {"$ref": 0}}),
        ] {
            assert!(decode(&bad.to_string(), usize::MAX).is_err(), "{bad}");
        }
    }
}
