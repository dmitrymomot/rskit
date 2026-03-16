use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::{self, Write};

modo::ulid_id!(SessionId);

/// Opaque session token (32 random bytes).
///
/// Stored in the browser cookie as a 64-character hex string; only the SHA-256
/// hash is persisted to the database so a compromised DB row cannot be replayed.
/// `Debug` and `Display` output are redacted (`****`) to prevent accidental
/// logging of the raw token.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SessionToken([u8; 32]);

impl SessionToken {
    /// Generate a cryptographically random 32-byte token.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        Self(bytes)
    }

    /// Decode a token from a 64-character lowercase hex string.
    ///
    /// Returns `Err` if `s` is not exactly 64 hex characters.
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

    /// Encode the raw token bytes as a 64-character lowercase hex string.
    pub fn as_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in &self.0 {
            write!(s, "{b:02x}").expect("writing to String cannot fail");
        }
        s
    }

    /// Compute the SHA-256 hash of the token as a 64-character lowercase hex string.
    ///
    /// This is the value stored in the database; the raw token is never persisted.
    pub fn hash(&self) -> String {
        let digest = Sha256::digest(self.0);
        let mut s = String::with_capacity(64);
        for b in digest {
            write!(s, "{b:02x}").expect("writing to String cannot fail");
        }
        s
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

impl Serialize for SessionToken {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_hex())
    }
}

impl<'de> Deserialize<'de> for SessionToken {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_hex(&s).map_err(serde::de::Error::custom)
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

/// Full session record loaded from the database.
///
/// Returned by [`crate::SessionManager::current`] and
/// [`crate::SessionManager::list_my_sessions`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    /// Unique session identifier (ULID).
    pub id: SessionId,
    #[serde(skip_serializing)]
    pub(crate) token_hash: String,
    /// ID of the authenticated user.
    pub user_id: String,
    /// Client IP address at session creation time.
    pub ip_address: String,
    /// Raw `User-Agent` header value.
    pub user_agent: String,
    /// Human-readable device name derived from the User-Agent (e.g. `"Chrome on macOS"`).
    pub device_name: String,
    /// Device category: `"desktop"`, `"mobile"`, or `"tablet"`.
    pub device_type: String,
    /// SHA-256 fingerprint of stable request headers used for hijack detection.
    pub fingerprint: String,
    /// Arbitrary JSON payload attached to the session.
    pub data: serde_json::Value,
    /// Timestamp when the session was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp of the last activity (updated on touch).
    pub last_active_at: DateTime<Utc>,
    /// Timestamp after which the session is considered expired.
    pub expires_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- SessionId tests ---

    #[test]
    fn session_id_generates_unique() {
        let a = SessionId::new();
        let b = SessionId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn session_id_ulid_format() {
        let id = SessionId::new();
        assert_eq!(id.as_str().len(), 26);
    }

    #[test]
    fn session_id_display_from_str_roundtrip() {
        let id = SessionId::new();
        let s = id.to_string();
        let parsed: SessionId = s.parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn session_id_from_raw() {
        let id = SessionId::from_raw("test-id");
        assert_eq!(id.as_str(), "test-id");
    }

    // --- SessionToken tests ---

    #[test]
    fn session_token_generates_64_hex() {
        let token = SessionToken::generate();
        let hex = token.as_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn session_token_unique() {
        let a = SessionToken::generate();
        let b = SessionToken::generate();
        assert_ne!(a, b);
    }

    #[test]
    fn session_token_from_hex_roundtrip() {
        let token = SessionToken::generate();
        let hex = token.as_hex();
        let parsed = SessionToken::from_hex(&hex).unwrap();
        assert_eq!(token, parsed);
    }

    #[test]
    fn session_token_from_hex_rejects_wrong_length() {
        assert!(SessionToken::from_hex("abcd").is_err());
    }

    #[test]
    fn session_token_from_hex_rejects_non_hex() {
        let bad = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        assert!(SessionToken::from_hex(bad).is_err());
    }

    #[test]
    fn session_token_hash_returns_64_hex() {
        let token = SessionToken::generate();
        let h = token.hash();
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn session_token_hash_deterministic() {
        let token = SessionToken::generate();
        assert_eq!(token.hash(), token.hash());
    }

    #[test]
    fn session_token_hash_differs_from_hex() {
        let token = SessionToken::generate();
        assert_ne!(token.hash(), token.as_hex());
    }

    #[test]
    fn session_token_debug_is_redacted() {
        let token = SessionToken::generate();
        let dbg = format!("{token:?}");
        assert_eq!(dbg, "SessionToken(****)");
        assert!(!dbg.contains(&token.as_hex()));
    }
}
