use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use std::fmt::Write;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// Generate a random token of `len` bytes, returned as a hex string.
pub fn generate(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    rand::rng().fill_bytes(&mut bytes);
    bytes_to_hex(&bytes)
}

/// Sign a token with HMAC-SHA256. Returns `{token}.{hmac_hex}`.
///
/// If `key` is empty (dev mode), returns the raw token unsigned.
pub fn sign(token: &str, key: &[u8]) -> String {
    if key.is_empty() {
        static WARN_ONCE: std::sync::Once = std::sync::Once::new();
        WARN_ONCE.call_once(|| {
            tracing::warn!("CSRF HMAC signing key is empty — tokens are unsigned (dev mode only)");
        });
        return token.to_string();
    }
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts any key length");
    mac.update(token.as_bytes());
    let result = mac.finalize().into_bytes();
    let sig = bytes_to_hex(&result);
    format!("{token}.{sig}")
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(hex, "{b:02x}").expect("writing to String cannot fail");
    }
    hex
}

/// Verify an HMAC-signed token. Returns the raw token on success.
///
/// If `key` is empty (dev mode), returns the input as-is (no signature expected).
pub fn verify(signed: &str, key: &[u8]) -> Option<String> {
    if key.is_empty() {
        static WARN_ONCE: std::sync::Once = std::sync::Once::new();
        WARN_ONCE.call_once(|| {
            tracing::warn!(
                "CSRF HMAC verification key is empty — signature check skipped (dev mode only)"
            );
        });
        return Some(signed.to_string());
    }
    let (token, sig_hex) = signed.rsplit_once('.')?;
    if token.is_empty() || sig_hex.is_empty() {
        return None;
    }

    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts any key length");
    mac.update(token.as_bytes());
    let expected = mac.finalize().into_bytes();

    let sig_bytes = hex_decode(sig_hex)?;
    if sig_bytes.len() != expected.len() {
        return None;
    }

    if sig_bytes.ct_eq(&expected).into() {
        Some(token.to_string())
    } else {
        None
    }
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(s.len() / 2);
    for chunk in s.as_bytes().chunks(2) {
        let hi = hex_digit(chunk[0])?;
        let lo = hex_digit(chunk[1])?;
        bytes.push((hi << 4) | lo);
    }
    Some(bytes)
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_correct_length() {
        let token = generate(32);
        assert_eq!(token.len(), 64); // 32 bytes = 64 hex chars
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_produces_unique_tokens() {
        let a = generate(32);
        let b = generate(32);
        assert_ne!(a, b);
    }

    #[test]
    fn sign_verify_roundtrip() {
        let key = b"test-secret-key";
        let token = generate(32);
        let signed = sign(&token, key);
        let verified = verify(&signed, key).unwrap();
        assert_eq!(verified, token);
    }

    #[test]
    fn verify_rejects_tampered_hmac() {
        let key = b"test-secret-key";
        let token = generate(32);
        let signed = sign(&token, key);
        // Tamper with the last character of the signature
        let mut tampered = signed.clone();
        let last = tampered.pop().unwrap();
        tampered.push(if last == 'a' { 'b' } else { 'a' });
        assert!(verify(&tampered, key).is_none());
    }

    #[test]
    fn verify_rejects_wrong_key() {
        let token = generate(32);
        let signed = sign(&token, b"key-a");
        assert!(verify(&signed, b"key-b").is_none());
    }

    #[test]
    fn verify_rejects_missing_signature() {
        assert!(verify("tokenonly", b"key").is_none());
    }

    #[test]
    fn verify_rejects_empty_parts() {
        assert!(verify(".abcd", b"key").is_none());
        assert!(verify("abcd.", b"key").is_none());
    }

    #[test]
    fn empty_key_skips_signing() {
        let token = generate(32);
        let signed = sign(&token, b"");
        assert_eq!(signed, token); // No dot, no signature
        let verified = verify(&signed, b"").unwrap();
        assert_eq!(verified, token);
    }
}
