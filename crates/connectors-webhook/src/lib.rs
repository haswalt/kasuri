//! Authentication for incoming events. Headers and secrets never enter engine inputs.
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
pub const MAX_WEBHOOK_BYTES: usize = 32_768;
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn hash(value: &[u8]) -> String {
    hex(&Sha256::digest(value))
}
pub fn valid_event_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
pub fn bearer(header: &str, expected_hash: &str) -> bool {
    header.strip_prefix("Bearer ").is_some_and(|token| {
        bool::from(
            hash(token.as_bytes())
                .as_bytes()
                .ct_eq(expected_hash.as_bytes()),
        )
    })
}
pub fn signature(secret: &[u8], timestamp: &str, event_id: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(event_id.as_bytes());
    mac.update(b".");
    mac.update(body);
    hex(&mac.finalize().into_bytes())
}
pub fn verify_signature(
    secret: &[u8],
    timestamp: &str,
    event_id: &str,
    body: &[u8],
    supplied: &str,
    now_seconds: u64,
) -> bool {
    if secret.is_empty()
        || !valid_event_id(event_id)
        || timestamp.len() > 20
        || !timestamp.bytes().all(|b| b.is_ascii_digit())
    {
        return false;
    }
    let Ok(time) = timestamp.parse::<u64>() else {
        return false;
    };
    if time.abs_diff(now_seconds) > 300 {
        return false;
    }
    let value = supplied.strip_prefix("sha256=").unwrap_or(supplied);
    bool::from(
        signature(secret, timestamp, event_id, body)
            .as_bytes()
            .ct_eq(value.as_bytes()),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bearer_and_signed_events_are_authenticated() {
        let secret = b"private";
        assert!(bearer("Bearer private", &hash(secret)));
        assert!(!bearer("Bearer wrong", &hash(secret)));
        let signed = signature(secret, "1000", "event-1", b"{}");
        assert!(verify_signature(
            secret, "1000", "event-1", b"{}", &signed, 1001
        ));
        assert!(!verify_signature(
            secret, "1000", "event-2", b"{}", &signed, 1001
        ));
        assert!(!verify_signature(
            secret, "1000", "event-1", b"changed", &signed, 1001
        ));
        assert!(!verify_signature(
            secret, "1000", "event-1", b"{}", &signed, 1301
        ));
    }
}
