//! [`SessionToken`] — opaque 32-byte cryptographic session token.
//!
//! The raw bytes are never transmitted; the hex-encoded value goes in the
//! signed cookie, and only the SHA-256 hash is stored in the database.
//! `Debug` and `Display` both redact the value as `"****"`.

use sha2::{Digest, Sha256};
use std::fmt;

/// A cryptographically random 32-byte session token.
///
/// The raw bytes are never transmitted; only the hex-encoded form is written
/// to the signed cookie, and the SHA-256 hash is stored in the database so
/// that a stolen database cannot be used to forge cookies.
///
/// `Debug` and `Display` both redact the value as `"****"` to prevent
/// accidental logging.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SessionToken([u8; 32]);

impl SessionToken {
    /// Generate a new random session token.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::fill(&mut bytes);
        Self(bytes)
    }

    /// Decode a session token from a 64-character lowercase hex string.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the string is not exactly 64 characters or contains
    /// non-hexadecimal characters.
    pub fn from_hex(s: &str) -> Result<Self, &'static str> {
        if s.len() != 64 {
            return Err("token must be 64 hex characters");
        }
        let mut bytes = [0u8; 32];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hi = hex_digit(chunk[0]).ok_or("invalid hex character")?;
            let lo = hex_digit(chunk[1]).ok_or("invalid hex character")?;
            bytes[i] = (hi << 4) | lo;
        }
        Ok(Self(bytes))
    }

    /// Encode the token as a 64-character lowercase hex string.
    ///
    /// This is the value written into the session cookie.
    pub fn as_hex(&self) -> String {
        crate::encoding::hex::encode(&self.0)
    }

    /// Compute the SHA-256 hash of the token and return it as a 64-character
    /// lowercase hex string.
    ///
    /// This hash is what is stored in `sessions.token_hash`. Storing only
    /// the hash ensures that a read of the database cannot be used to impersonate
    /// users.
    pub fn hash(&self) -> String {
        let digest = Sha256::digest(self.0);
        crate::encoding::hex::encode(&digest)
    }

    /// Expose the raw token as a 64-character hex string.
    ///
    /// This intentionally breaks the redaction guarantee and is meant only for
    /// JWT `jti` round-tripping inside the crate. Do not use for logging.
    pub fn expose(&self) -> String {
        self.as_hex()
    }

    /// Reconstruct a `SessionToken` from a 64-character hex string (the value
    /// previously returned by [`expose`](Self::expose)).
    ///
    /// Returns `None` if the string is not a valid 64-character hex encoding.
    pub fn from_raw(s: &str) -> Option<Self> {
        Self::from_hex(s).ok()
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SessionToken(****)")
    }
}

impl fmt::Display for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("****")
    }
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
