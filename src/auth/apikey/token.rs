use crate::auth::internal::{random_string, verify_sha256_hex};
use crate::encoding::hex;

const BASE62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const ULID_LEN: usize = 26;

/// Result of parsing a raw API key token.
pub(crate) struct ParsedToken<'a> {
    /// The ULID portion (26 chars), used as the database primary key.
    pub id: &'a str,
    /// The secret portion (remaining chars after the ULID).
    pub secret: &'a str,
}

/// Generate a random base62 secret of `len` characters.
pub(crate) fn generate_secret(len: usize) -> String {
    random_string(BASE62, len)
}

/// Format a full token: `{prefix}_{ulid}{secret}`.
pub(crate) fn format_token(prefix: &str, ulid: &str, secret: &str) -> String {
    format!("{prefix}_{ulid}{secret}")
}

/// Parse a raw token into its ULID and secret components.
///
/// Returns `None` if the token format is invalid or the prefix doesn't match.
pub(crate) fn parse_token<'a>(raw: &'a str, expected_prefix: &str) -> Option<ParsedToken<'a>> {
    let (prefix, body) = raw.split_once('_')?;
    if prefix != expected_prefix {
        return None;
    }
    if body.len() <= ULID_LEN {
        return None;
    }
    let (id, secret) = body.split_at(ULID_LEN);
    Some(ParsedToken { id, secret })
}

/// SHA-256 hash of a secret, returned as a 64-char lowercase hex string.
pub(crate) fn hash_secret(secret: &str) -> String {
    hex::sha256(secret.as_bytes())
}

/// Constant-time comparison of a secret against a stored hash.
pub(crate) fn verify_hash(secret: &str, stored_hash: &str) -> bool {
    verify_sha256_hex(secret, stored_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_secret_correct_length() {
        let secret = generate_secret(32);
        assert_eq!(secret.len(), 32);
    }

    #[test]
    fn generate_secret_is_base62() {
        let secret = generate_secret(32);
        assert!(
            secret.chars().all(|c| c.is_ascii_alphanumeric()),
            "secret contains non-base62 chars: {secret}"
        );
    }

    #[test]
    fn generate_secret_unique() {
        let a = generate_secret(32);
        let b = generate_secret(32);
        assert_ne!(a, b);
    }

    #[test]
    fn format_token_structure() {
        let token = format_token("modo", "01JQXK5M3N8R4T6V2W9Y0ZABCD", "secret123");
        assert_eq!(token, "modo_01JQXK5M3N8R4T6V2W9Y0ZABCDsecret123");
    }

    #[test]
    fn parse_token_roundtrip() {
        let token = format_token("modo", "01JQXK5M3N8R4T6V2W9Y0ZABCD", "abcdefghij");
        let parsed = parse_token(&token, "modo").unwrap();
        assert_eq!(parsed.id, "01JQXK5M3N8R4T6V2W9Y0ZABCD");
        assert_eq!(parsed.secret, "abcdefghij");
    }

    #[test]
    fn parse_token_wrong_prefix() {
        let token = "sk_01JQXK5M3N8R4T6V2W9Y0ZABCDsecret";
        assert!(parse_token(token, "modo").is_none());
    }

    #[test]
    fn parse_token_no_underscore() {
        assert!(parse_token("nounderscore", "modo").is_none());
    }

    #[test]
    fn parse_token_body_too_short() {
        // Body shorter than 26 chars (ULID length) — no secret portion
        let token = "modo_SHORT";
        assert!(parse_token(token, "modo").is_none());
    }

    #[test]
    fn hash_secret_produces_64_char_hex() {
        let hash = hash_secret("testsecret");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_secret_deterministic() {
        let a = hash_secret("same");
        let b = hash_secret("same");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_secret_different_inputs_differ() {
        let a = hash_secret("one");
        let b = hash_secret("two");
        assert_ne!(a, b);
    }

    #[test]
    fn verify_hash_correct_secret() {
        let hash = hash_secret("mysecret");
        assert!(verify_hash("mysecret", &hash));
    }

    #[test]
    fn verify_hash_wrong_secret() {
        let hash = hash_secret("mysecret");
        assert!(!verify_hash("wrong", &hash));
    }
}
