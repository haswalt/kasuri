//! Public metadata for private workflow files. File bytes remain in the host's bucket.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Clairic accepts at most four media inputs, each 4 MiB and 8 MiB combined.
pub const MAX_MEDIA_ITEMS: usize = 4;
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_MEDIA_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileReference {
    pub id: String,
    pub name: String,
    pub content_type: String,
    pub size: u64,
}

impl FileReference {
    pub fn valid(&self) -> bool {
        valid_id(&self.id)
            && !self.name.is_empty()
            && self.name.len() <= 255
            && !self.name.chars().any(char::is_control)
            && matches!(
                self.content_type.as_str(),
                "application/pdf" | "image/png" | "image/jpeg" | "image/webp"
            )
            && (1..=MAX_FILE_BYTES).contains(&self.size)
    }
    pub fn schema() -> Value {
        json!({"type":"object","additionalProperties":false,
            "properties":{
                "id":{"type":"string","pattern":"^[0-9a-f]{64}$"},
                "name":{"type":"string"},
                "contentType":{"enum":["application/pdf","image/png","image/jpeg","image/webp"]},
                "size":{"type":"integer","minimum":1,"maximum":MAX_FILE_BYTES}
            },"required":["id","name","contentType","size"]})
    }
}

/// Generated IDs are opaque, never supplied as bucket keys or paths.
pub fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn detect_media(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"%PDF-") {
        Some("application/pdf")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    }
}
