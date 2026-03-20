use sha2::{Digest, Sha256};
use std::fmt::Write;

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
    let mut s = String::with_capacity(64);
    for b in digest {
        write!(s, "{b:02x}").expect("writing to String cannot fail");
    }
    s
}
