use subtle::ConstantTimeEq;

/// Generates a numeric one-time password of `length` digits.
///
/// Uses rejection sampling over `OsRng` to avoid modulo bias. Returns a tuple
/// of `(plaintext_code, sha256_hex_hash)`. Store only the hash; send the
/// plaintext to the user. Verify later with [`verify`].
///
/// Requires feature `"auth"`.
pub fn generate(length: usize) -> (String, String) {
    let mut code = String::with_capacity(length);
    for _ in 0..length {
        let mut byte = [0u8; 1];
        loop {
            rand::fill(&mut byte);
            // Rejection sampling: only accept values 0-249 to avoid modulo bias
            if byte[0] < 250 {
                code.push((b'0' + (byte[0] % 10)) as char);
                break;
            }
        }
    }
    let hash = sha256_hex(&code);
    (code, hash)
}

/// Verifies `code` against a SHA-256 hex `hash` produced by [`generate`].
///
/// Comparison is constant-time to prevent timing attacks.
///
/// Requires feature `"auth"`.
pub fn verify(code: &str, hash: &str) -> bool {
    let computed = sha256_hex(code);
    computed.as_bytes().ct_eq(hash.as_bytes()).into()
}

fn sha256_hex(input: &str) -> String {
    crate::encoding::hex::sha256(input)
}
