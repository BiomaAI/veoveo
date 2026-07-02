//! WaveSpeed webhook signature verification.
//!
//! WaveSpeed signs callbacks with HMAC-SHA256 over `{webhook-id}.{webhook-timestamp}.{body}`
//! using the account webhook secret (the `whsec_` prefix is stripped, the remainder is used
//! as the raw HMAC key — not base64-decoded). The signature arrives in the
//! `webhook-signature` header as `v3,<hex>`.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, PartialEq)]
pub enum VerifyError {
    MissingHeader(&'static str),
    BadSignatureFormat,
    SignatureMismatch,
    TimestampSkew { skew_secs: i64 },
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::MissingHeader(h) => write!(f, "missing header: {h}"),
            VerifyError::BadSignatureFormat => write!(f, "bad signature format"),
            VerifyError::SignatureMismatch => write!(f, "signature mismatch"),
            VerifyError::TimestampSkew { skew_secs } => {
                write!(f, "timestamp skew too large: {skew_secs}s")
            }
        }
    }
}
impl std::error::Error for VerifyError {}

/// Verify a WaveSpeed webhook. `tolerance_secs` bounds replay-attack windows;
/// pass `None` to skip the timestamp check (useful in tests).
pub fn verify(
    secret: &str,
    webhook_id: &str,
    webhook_timestamp: &str,
    body: &[u8],
    signature_header: &str,
    tolerance_secs: Option<i64>,
) -> Result<(), VerifyError> {
    if let Some(tolerance) = tolerance_secs {
        let ts: i64 = webhook_timestamp
            .parse()
            .map_err(|_| VerifyError::BadSignatureFormat)?;
        let now = chrono::Utc::now().timestamp();
        let skew = (now - ts).abs();
        if skew > tolerance {
            return Err(VerifyError::TimestampSkew { skew_secs: skew });
        }
    }

    let key = secret.strip_prefix("whsec_").unwrap_or(secret);
    let mut payload = Vec::with_capacity(webhook_id.len() + webhook_timestamp.len() + body.len() + 2);
    payload.extend_from_slice(webhook_id.as_bytes());
    payload.push(b'.');
    payload.extend_from_slice(webhook_timestamp.as_bytes());
    payload.push(b'.');
    payload.extend_from_slice(body);

    // The header may carry multiple space-separated signatures; accept any v3 match.
    for part in signature_header.split_whitespace() {
        let Some(hex_sig) = part.strip_prefix("v3,") else {
            continue;
        };
        let Ok(sig_bytes) = hex::decode(hex_sig) else {
            continue;
        };
        let mut mac = HmacSha256::new_from_slice(key.as_bytes())
            .expect("hmac accepts any key length");
        mac.update(&payload);
        if mac.verify_slice(&sig_bytes).is_ok() {
            return Ok(());
        }
    }
    Err(VerifyError::SignatureMismatch)
}

/// Compute the `v3,<hex>` signature for a payload — used by tests and local tooling.
pub fn sign(secret: &str, webhook_id: &str, webhook_timestamp: &str, body: &[u8]) -> String {
    let key = secret.strip_prefix("whsec_").unwrap_or(secret);
    let mut mac =
        HmacSha256::new_from_slice(key.as_bytes()).expect("hmac accepts any key length");
    mac.update(webhook_id.as_bytes());
    mac.update(b".");
    mac.update(webhook_timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    format!("v3,{}", hex::encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "whsec_test-secret-key";

    #[test]
    fn round_trip_verifies() {
        let body = br#"{"id":"abc","status":"completed"}"#;
        let sig = sign(SECRET, "msg_1", "1700000000", body);
        assert_eq!(
            verify(SECRET, "msg_1", "1700000000", body, &sig, None),
            Ok(())
        );
    }

    #[test]
    fn tampered_body_fails() {
        let sig = sign(SECRET, "msg_1", "1700000000", b"original");
        assert_eq!(
            verify(SECRET, "msg_1", "1700000000", b"tampered", &sig, None),
            Err(VerifyError::SignatureMismatch)
        );
    }

    #[test]
    fn wrong_prefix_ignored() {
        let sig = sign(SECRET, "msg_1", "1700000000", b"x").replace("v3,", "v1,");
        assert_eq!(
            verify(SECRET, "msg_1", "1700000000", b"x", &sig, None),
            Err(VerifyError::SignatureMismatch)
        );
    }

    #[test]
    fn stale_timestamp_rejected() {
        let body = b"x";
        let sig = sign(SECRET, "msg_1", "1700000000", body);
        assert!(matches!(
            verify(SECRET, "msg_1", "1700000000", body, &sig, Some(300)),
            Err(VerifyError::TimestampSkew { .. })
        ));
    }
}
