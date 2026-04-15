//! Crate-private helpers shared across `auth` submodules.

use subtle::ConstantTimeEq;

/// Constant-time SHA-256 hex comparison: `sha256_hex(plain) == stored_hex`.
pub(crate) fn verify_sha256_hex(plain: impl AsRef<[u8]>, stored_hex: &str) -> bool {
    let computed = crate::encoding::hex::sha256(plain.as_ref());
    computed.as_bytes().ct_eq(stored_hex.as_bytes()).into()
}

/// Generate a random string by sampling `len` characters from `alphabet` using
/// rejection sampling to avoid modulo bias.
///
/// Panics if `alphabet` is empty.
pub(crate) fn random_string(alphabet: &[u8], len: usize) -> String {
    assert!(!alphabet.is_empty(), "alphabet must not be empty");
    let n = alphabet.len();
    let bias_limit = ((256 / n) * n) as u8;
    let mut out = String::with_capacity(len);
    let mut buf = [0u8; 1];
    while out.len() < len {
        rand::fill(&mut buf[..]);
        let b = buf[0];
        if b < bias_limit {
            out.push(alphabet[(b as usize) % n] as char);
        }
    }
    out
}
