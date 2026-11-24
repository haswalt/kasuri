//! Authentication for incoming events. Headers and secrets never enter engine inputs.
use hmac::{Hmac, KeyInit, Mac};
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
    /// Hashes and signatures are persisted (token hashes, receipts, run IDs) and
    /// checked by external senders, so their encoding must never change across
    /// dependency upgrades. Standard vectors pin both.
    #[test]
    fn digests_and_signatures_match_standard_vectors() {
        assert_eq!(
            hash(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // RFC 4231 test case 2.
        let mut mac = Hmac::<Sha256>::new_from_slice(b"Jefe").unwrap();
        mac.update(b"what do ya want for nothing?");
        assert_eq!(
            hex(&mac.finalize().into_bytes()),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        // The v2 signature is HMAC-SHA256 over timestamp + "." + body.
        let mut mac = Hmac::<Sha256>::new_from_slice(b"secret").unwrap();
        mac.update(b"1700000000.{\"a\":1}");
        assert_eq!(
            signature_v2(b"secret", "1700000000", b"{\"a\":1}"),
            hex(&mac.finalize().into_bytes())
        );
    }
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

pub fn token(value: &str, expected_hash: &str) -> bool {
    bool::from(
        hash(value.as_bytes())
            .as_bytes()
            .ct_eq(expected_hash.as_bytes()),
    )
}
pub fn valid_parameter(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
/// Signature format without a business event ID: timestamp + '.' + raw body.
pub fn signature_v2(secret: &[u8], timestamp: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    hex(&mac.finalize().into_bytes())
}
pub fn verify_signature_v2(
    secret: &[u8],
    timestamp: &str,
    body: &[u8],
    supplied: &str,
    now_seconds: u64,
) -> bool {
    if secret.is_empty()
        || timestamp.is_empty()
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
    bool::from(
        signature_v2(secret, timestamp, body).as_bytes().ct_eq(
            supplied
                .strip_prefix("sha256=")
                .unwrap_or(supplied)
                .as_bytes(),
        ),
    )
}

#[cfg(test)]
mod modern_tests {
    use super::*;
    #[test]
    fn event_free_hmac_binds_body_and_timestamp_with_a_bounded_window() {
        let signed = signature_v2(b"private", "1000", b"{}");
        assert!(verify_signature_v2(
            b"private", "1000", b"{}", &signed, 1001
        ));
        assert!(!verify_signature_v2(
            b"private", "1000", b"changed", &signed, 1001
        ));
        assert!(!verify_signature_v2(
            b"private", "1001", b"{}", &signed, 1001
        ));
        assert!(!verify_signature_v2(
            b"private", "1000", b"{}", &signed, 1301
        ));
        assert!(!verify_signature_v2(b"private", "", b"{}", &signed, 1000));
        assert!(verify_signature_v2(
            b"private",
            "1000",
            b"{}",
            &format!("sha256={signed}"),
            1000
        ));
        assert!(!verify_signature(
            b"private", "1000", "event", b"{}", &signed, 1000
        ));
    }
}

pub mod context;
pub mod response;
