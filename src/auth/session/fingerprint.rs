//! Browser fingerprinting for session hijacking detection.

use sha2::{Digest, Sha256};

/// Compute a SHA-256 fingerprint from three request headers.
///
/// The headers are concatenated with null-byte separators before hashing to
/// prevent boundary confusion. Returns the fingerprint as a 64-character
/// lowercase hex string.
///
/// Used by [`super::cookie::CookieSessionLayer`] to detect potential session
/// hijacking when the session configuration has fingerprint validation enabled.
pub fn compute_fingerprint(
    user_agent: &str,
    accept_language: &str,
    accept_encoding: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(user_agent.as_bytes());
    hasher.update(b"\x00");
    hasher.update(accept_language.as_bytes());
    hasher.update(b"\x00");
    hasher.update(accept_encoding.as_bytes());
    let digest = hasher.finalize();
    crate::encoding::hex::encode(&digest)
}
