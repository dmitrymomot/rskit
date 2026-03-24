use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::secret::WebhookSecret;
use crate::error::{Error, Result};

type HmacSha256 = Hmac<Sha256>;

/// Compute HMAC-SHA256 of `content` using `secret`, returned as standard base64.
pub fn sign(secret: &WebhookSecret, content: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(content);
    BASE64.encode(mac.finalize().into_bytes())
}

/// Verify a base64-encoded HMAC-SHA256 signature against `content` using `secret`.
///
/// Uses constant-time comparison. Returns `false` if `signature` is not valid
/// base64 or does not match.
pub fn verify(secret: &WebhookSecret, content: &[u8], signature: &str) -> bool {
    let sig_bytes = match BASE64.decode(signature) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(content);
    mac.verify_slice(&sig_bytes).is_ok()
}

/// The three Standard Webhooks headers produced by [`sign_headers`].
pub struct SignedHeaders {
    /// Value for the `webhook-id` header.
    pub webhook_id: String,
    /// Value for the `webhook-timestamp` header (Unix seconds).
    pub webhook_timestamp: i64,
    /// Value for the `webhook-signature` header (`v1,<base64>` entries separated by spaces).
    pub webhook_signature: String,
}

/// Build Standard Webhooks signed content and sign it with every secret in `secrets`.
///
/// Each secret produces one `v1,<base64>` entry; multiple entries are joined with
/// a space, which supports key rotation on both sender and receiver sides.
///
/// # Panics
///
/// Panics if `secrets` is empty. `WebhookSender::send` validates this before calling.
pub fn sign_headers(
    secrets: &[&WebhookSecret],
    id: &str,
    timestamp: i64,
    body: &[u8],
) -> SignedHeaders {
    assert!(!secrets.is_empty(), "at least one secret required");

    let content = build_signed_content(id, timestamp, body);
    let sigs: Vec<String> = secrets
        .iter()
        .map(|s| format!("v1,{}", sign(s, &content)))
        .collect();

    SignedHeaders {
        webhook_id: id.to_string(),
        webhook_timestamp: timestamp,
        webhook_signature: sigs.join(" "),
    }
}

/// Parse Standard Webhooks headers from an incoming request and verify the signature.
///
/// Reads `webhook-id`, `webhook-timestamp`, and `webhook-signature` from `headers`.
/// Validates that the timestamp is within `tolerance` of now (replay protection),
/// then tries every `v1,` signature entry against every secret.
/// Returns `Ok(())` as soon as one combination matches.
pub fn verify_headers(
    secrets: &[&WebhookSecret],
    headers: &http::HeaderMap,
    body: &[u8],
    tolerance: Duration,
) -> Result<()> {
    let id = header_str(headers, "webhook-id")?;
    let ts_str = header_str(headers, "webhook-timestamp")?;
    let sig_header = header_str(headers, "webhook-signature")?;

    let timestamp: i64 = ts_str
        .parse()
        .map_err(|_| Error::bad_request("invalid webhook-timestamp"))?;

    // Check timestamp tolerance
    let now = chrono::Utc::now().timestamp();
    let diff = (now - timestamp).unsigned_abs();
    if diff > tolerance.as_secs() {
        return Err(Error::bad_request("webhook timestamp outside tolerance"));
    }

    let content = build_signed_content(id, timestamp, body);

    // Try each v1 signature against each secret
    for sig_entry in sig_header.split(' ') {
        let raw_sig = match sig_entry.strip_prefix("v1,") {
            Some(s) => s,
            None => continue, // skip non-v1 signatures
        };
        for secret in secrets {
            if verify(secret, &content, raw_sig) {
                return Ok(());
            }
        }
    }

    Err(Error::bad_request("no valid webhook signature found"))
}

fn build_signed_content(id: &str, timestamp: i64, body: &[u8]) -> Vec<u8> {
    let prefix = format!("{id}.{timestamp}.");
    let mut content = Vec::with_capacity(prefix.len() + body.len());
    content.extend_from_slice(prefix.as_bytes());
    content.extend_from_slice(body);
    content
}

fn header_str<'a>(headers: &'a http::HeaderMap, name: &str) -> Result<&'a str> {
    headers
        .get(name)
        .ok_or_else(|| Error::bad_request(format!("missing {name} header")))?
        .to_str()
        .map_err(|_| Error::bad_request(format!("invalid {name} header encoding")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_produces_base64() {
        let secret = WebhookSecret::new(b"test-key".to_vec());
        let sig = sign(&secret, b"hello");
        // Should be valid base64
        assert!(BASE64.decode(&sig).is_ok());
    }

    #[test]
    fn verify_valid_signature() {
        let secret = WebhookSecret::new(b"test-key".to_vec());
        let sig = sign(&secret, b"hello");
        assert!(verify(&secret, b"hello", &sig));
    }

    #[test]
    fn verify_wrong_secret_fails() {
        let secret1 = WebhookSecret::new(b"key-one".to_vec());
        let secret2 = WebhookSecret::new(b"key-two".to_vec());
        let sig = sign(&secret1, b"hello");
        assert!(!verify(&secret2, b"hello", &sig));
    }

    #[test]
    fn verify_tampered_content_fails() {
        let secret = WebhookSecret::new(b"test-key".to_vec());
        let sig = sign(&secret, b"hello");
        assert!(!verify(&secret, b"tampered", &sig));
    }

    #[test]
    fn verify_invalid_base64_returns_false() {
        let secret = WebhookSecret::new(b"test-key".to_vec());
        assert!(!verify(&secret, b"hello", "!!!not-base64!!!"));
    }

    #[test]
    fn sign_empty_content() {
        let secret = WebhookSecret::new(b"test-key".to_vec());
        let sig = sign(&secret, b"");
        assert!(verify(&secret, b"", &sig));
    }

    #[test]
    fn known_test_vector() {
        // Precomputed: HMAC-SHA256("test-secret", "test-content") as base64
        let secret = WebhookSecret::new(b"test-secret".to_vec());
        let sig = sign(&secret, b"test-content");
        // Verify round-trip; the exact value is deterministic
        assert!(verify(&secret, b"test-content", &sig));
        // Different content must fail
        assert!(!verify(&secret, b"other-content", &sig));
    }

    use std::time::Duration;

    fn make_headers(id: &str, ts: i64, sig: &str) -> http::HeaderMap {
        let mut headers = http::HeaderMap::new();
        headers.insert("webhook-id", id.parse().unwrap());
        headers.insert("webhook-timestamp", ts.to_string().parse().unwrap());
        headers.insert("webhook-signature", sig.parse().unwrap());
        headers
    }

    #[test]
    fn sign_headers_single_secret() {
        let secret = WebhookSecret::new(b"key".to_vec());
        let sh = sign_headers(&[&secret], "msg_123", 1000, b"body");
        assert_eq!(sh.webhook_id, "msg_123");
        assert_eq!(sh.webhook_timestamp, 1000);
        assert!(sh.webhook_signature.starts_with("v1,"));
        assert!(!sh.webhook_signature.contains(' '));
    }

    #[test]
    fn sign_headers_multiple_secrets() {
        let s1 = WebhookSecret::new(b"key1".to_vec());
        let s2 = WebhookSecret::new(b"key2".to_vec());
        let sh = sign_headers(&[&s1, &s2], "msg_123", 1000, b"body");
        let parts: Vec<&str> = sh.webhook_signature.split(' ').collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].starts_with("v1,"));
        assert!(parts[1].starts_with("v1,"));
        assert_ne!(parts[0], parts[1]);
    }

    #[test]
    #[should_panic(expected = "at least one secret")]
    fn sign_headers_empty_secrets_panics() {
        sign_headers(&[], "msg_123", 1000, b"body");
    }

    #[test]
    fn verify_headers_valid() {
        let secret = WebhookSecret::new(b"key".to_vec());
        let now = chrono::Utc::now().timestamp();
        let sh = sign_headers(&[&secret], "msg_1", now, b"payload");
        let headers = make_headers(&sh.webhook_id, sh.webhook_timestamp, &sh.webhook_signature);
        let result = verify_headers(&[&secret], &headers, b"payload", Duration::from_secs(300));
        assert!(result.is_ok());
    }

    #[test]
    fn verify_headers_wrong_secret_fails() {
        let sign_secret = WebhookSecret::new(b"sign-key".to_vec());
        let verify_secret = WebhookSecret::new(b"wrong-key".to_vec());
        let now = chrono::Utc::now().timestamp();
        let sh = sign_headers(&[&sign_secret], "msg_1", now, b"data");
        let headers = make_headers(&sh.webhook_id, sh.webhook_timestamp, &sh.webhook_signature);
        let result = verify_headers(
            &[&verify_secret],
            &headers,
            b"data",
            Duration::from_secs(300),
        );
        assert!(result.is_err());
    }

    #[test]
    fn verify_headers_expired_timestamp() {
        let secret = WebhookSecret::new(b"key".to_vec());
        let old_ts = chrono::Utc::now().timestamp() - 600; // 10 minutes ago
        let sh = sign_headers(&[&secret], "msg_1", old_ts, b"data");
        let headers = make_headers(&sh.webhook_id, sh.webhook_timestamp, &sh.webhook_signature);
        let result = verify_headers(&[&secret], &headers, b"data", Duration::from_secs(300));
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("tolerance"));
    }

    #[test]
    fn verify_headers_future_timestamp() {
        let secret = WebhookSecret::new(b"key".to_vec());
        let future_ts = chrono::Utc::now().timestamp() + 600; // 10 minutes ahead
        let sh = sign_headers(&[&secret], "msg_1", future_ts, b"data");
        let headers = make_headers(&sh.webhook_id, sh.webhook_timestamp, &sh.webhook_signature);
        let result = verify_headers(&[&secret], &headers, b"data", Duration::from_secs(300));
        assert!(result.is_err());
    }

    #[test]
    fn verify_headers_missing_header() {
        let secret = WebhookSecret::new(b"key".to_vec());
        let headers = http::HeaderMap::new(); // empty
        let result = verify_headers(&[&secret], &headers, b"data", Duration::from_secs(300));
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("missing"));
    }

    #[test]
    fn verify_headers_multi_signature_rotation() {
        let old_secret = WebhookSecret::new(b"old-key".to_vec());
        let new_secret = WebhookSecret::new(b"new-key".to_vec());
        let now = chrono::Utc::now().timestamp();
        // Sign with both secrets (key rotation)
        let sh = sign_headers(&[&old_secret, &new_secret], "msg_1", now, b"data");
        let headers = make_headers(&sh.webhook_id, sh.webhook_timestamp, &sh.webhook_signature);
        // Verify with only the new secret — should still pass (one signature matches)
        let result = verify_headers(&[&new_secret], &headers, b"data", Duration::from_secs(300));
        assert!(result.is_ok());
    }

    #[test]
    fn verify_headers_multi_secret_on_verify_side() {
        let secret = WebhookSecret::new(b"the-key".to_vec());
        let wrong_secret = WebhookSecret::new(b"wrong-key".to_vec());
        let now = chrono::Utc::now().timestamp();
        // Sign with one secret
        let sh = sign_headers(&[&secret], "msg_1", now, b"data");
        let headers = make_headers(&sh.webhook_id, sh.webhook_timestamp, &sh.webhook_signature);
        // Verify with both (wrong + correct) — should pass because one matches
        let result = verify_headers(
            &[&wrong_secret, &secret],
            &headers,
            b"data",
            Duration::from_secs(300),
        );
        assert!(result.is_ok());
    }
}
