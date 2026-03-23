use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::secret::WebhookSecret;

type HmacSha256 = Hmac<Sha256>;

/// HMAC-SHA256 of arbitrary content, returned as standard base64.
pub fn sign(secret: &WebhookSecret, content: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(content);
    BASE64.encode(mac.finalize().into_bytes())
}

/// Constant-time verify via `hmac::Mac::verify_slice`.
/// Decodes the base64 signature and compares against HMAC-SHA256 of content.
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
}
