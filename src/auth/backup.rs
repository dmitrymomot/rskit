use super::internal::{random_string, verify_sha256_hex};

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// Generates `count` one-time backup recovery codes.
///
/// Each code is formatted as `xxxx-xxxx` (8 lowercase alphanumeric characters
/// split by a hyphen). Returns a `Vec` of `(plaintext_code, sha256_hex_hash)`
/// tuples. Store only the hashes; display the plaintext codes to the user
/// once. Verify a submitted code with [`verify`].
///
/// Uses rejection sampling over `OsRng` to avoid modulo bias.
pub fn generate(count: usize) -> Vec<(String, String)> {
    (0..count).map(|_| generate_one()).collect()
}

/// Verifies `code` against a SHA-256 hex `hash` produced by [`generate`].
///
/// Normalizes `code` before hashing (strips hyphens, lowercases) so that
/// users can submit codes with or without the separator. Comparison is
/// constant-time to prevent timing attacks.
pub fn verify(code: &str, hash: &str) -> bool {
    verify_sha256_hex(normalize(code), hash)
}

fn generate_one() -> (String, String) {
    let raw = random_string(ALPHABET, 8);
    let plaintext = format!("{}-{}", &raw[..4], &raw[4..]);
    let hash = crate::encoding::hex::sha256(normalize(&plaintext));
    (plaintext, hash)
}

fn normalize(code: &str) -> String {
    code.replace('-', "").to_lowercase()
}
