use sha2::{Digest, Sha256};

/// Compute a server-side fingerprint from stable request attributes.
/// Excludes IP (changes on mobile network switches).
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
    hex_encode::encode(hasher.finalize())
}

// sha2 outputs bytes; we need hex encoding. Use a simple inline hex encoder
// to avoid adding the `hex` crate.
mod hex_encode {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        use std::fmt::Write;
        let bytes = bytes.as_ref();
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            write!(s, "{b:02x}").expect("writing to String cannot fail");
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_deterministic() {
        let a = compute_fingerprint("Mozilla/5.0", "en-US", "gzip");
        let b = compute_fingerprint("Mozilla/5.0", "en-US", "gzip");
        assert_eq!(a, b);
    }

    #[test]
    fn fingerprint_differs_on_different_input() {
        let a = compute_fingerprint("Mozilla/5.0", "en-US", "gzip");
        let b = compute_fingerprint("Mozilla/5.0", "fr-FR", "gzip");
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_is_sha256_hex() {
        let fp = compute_fingerprint("test", "en", "gzip");
        assert_eq!(fp.len(), 64); // SHA256 = 32 bytes = 64 hex chars
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn fingerprint_separator_prevents_collision() {
        // Without a separator, ("ab", "cd", "ef") and ("abc", "de", "f") would hash identically
        let a = compute_fingerprint("ab", "cd", "ef");
        let b = compute_fingerprint("abc", "de", "f");
        assert_ne!(a, b);
    }
}
