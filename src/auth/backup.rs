use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// Generates `count` one-time backup recovery codes.
///
/// Each code is formatted as `xxxx-xxxx` (8 lowercase alphanumeric characters
/// split by a hyphen). Returns a `Vec` of `(plaintext_code, sha256_hex_hash)`
/// tuples. Store only the hashes; display the plaintext codes to the user
/// once. Verify a submitted code with [`verify`].
///
/// Uses rejection sampling over `OsRng` to avoid modulo bias.
///
/// Requires feature `"auth"`.
pub fn generate(count: usize) -> Vec<(String, String)> {
    (0..count).map(|_| generate_one()).collect()
}

/// Verifies `code` against a SHA-256 hex `hash` produced by [`generate`].
///
/// Normalizes `code` before hashing (strips hyphens, lowercases) so that
/// users can submit codes with or without the separator. Comparison is
/// constant-time to prevent timing attacks.
///
/// Requires feature `"auth"`.
pub fn verify(code: &str, hash: &str) -> bool {
    let normalized = normalize(code);
    let computed = sha256_hex(&normalized);
    computed.as_bytes().ct_eq(hash.as_bytes()).into()
}

fn generate_one() -> (String, String) {
    let mut chars = Vec::with_capacity(8);
    for _ in 0..8 {
        let mut byte = [0u8; 1];
        loop {
            rand::fill(&mut byte);
            // Rejection sampling: ALPHABET.len()=36, accept <252 to avoid modulo bias (252 = 36*7)
            if byte[0] < 252 {
                chars.push(ALPHABET[(byte[0] as usize) % ALPHABET.len()] as char);
                break;
            }
        }
    }

    let plaintext = format!(
        "{}-{}",
        chars[..4].iter().collect::<String>(),
        chars[4..].iter().collect::<String>(),
    );
    let hash = sha256_hex(&normalize(&plaintext));
    (plaintext, hash)
}

fn normalize(code: &str) -> String {
    code.replace('-', "").to_lowercase()
}

fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    crate::encoding::hex::encode(&digest)
}
