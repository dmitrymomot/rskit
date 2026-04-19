use super::internal::{random_string, verify_sha256_hex};

const DIGITS: &[u8] = b"0123456789";

/// Generates a numeric one-time password of `length` digits.
///
/// Uses rejection sampling over `OsRng` to avoid modulo bias. Returns a tuple
/// of `(plaintext_code, sha256_hex_hash)`. Store only the hash; send the
/// plaintext to the user. Verify later with [`verify`].
pub fn generate(length: usize) -> (String, String) {
    let code = random_string(DIGITS, length);
    let hash = crate::encoding::hex::sha256(&code);
    (code, hash)
}

/// Verifies `code` against a SHA-256 hex `hash` produced by [`generate`].
///
/// Comparison is constant-time to prevent timing attacks.
pub fn verify(code: &str, hash: &str) -> bool {
    verify_sha256_hex(code, hash)
}
