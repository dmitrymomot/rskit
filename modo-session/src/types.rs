use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::{self, Write};
use std::str::FromStr;

/// Opaque session identifier (ULID string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    pub fn new() -> Self {
        Self(ulid::Ulid::new().to_string())
    }

    pub fn from_raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for SessionId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

/// Opaque session token (32 random bytes).
///
/// Stored in cookie; only the SHA256 hash is persisted to DB.
/// Debug output is redacted to prevent accidental logging.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SessionToken([u8; 32]);

impl SessionToken {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        Self(bytes)
    }

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

    pub fn as_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in &self.0 {
            write!(s, "{b:02x}").expect("writing to String cannot fail");
        }
        s
    }

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

/// Full session record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub id: SessionId,
    pub(crate) token_hash: String,
    pub user_id: String,
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,
    pub device_type: String,
    pub fingerprint: String,
    pub data: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
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
