use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Write};

/// Opaque session identifier (ULID string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionId {
    /// Generate a new ULID-based session ID.
    pub fn new() -> Self {
        Self(ulid::Ulid::new().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Opaque session token (32 random bytes, hex-encoded).
/// This is the value stored in the encrypted cookie — rotatable independently of SessionId.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionToken(String);

impl SessionToken {
    /// Generate a new cryptographically random session token.
    pub fn generate() -> Self {
        let bytes: [u8; 32] = rand::rng().random();
        let mut s = String::with_capacity(64);
        for b in bytes {
            write!(s, "{b:02x}").expect("writing to String cannot fail");
        }
        Self(s)
    }

    /// Wrap a raw token string (from cookie value).
    pub(crate) fn from_raw(s: String) -> Self {
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.len() >= 4 {
            write!(f, "SessionToken(****...{})", &self.0[self.0.len() - 4..])
        } else {
            write!(f, "SessionToken(****)")
        }
    }
}

impl fmt::Display for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.len() >= 4 {
            write!(f, "****...{}", &self.0[self.0.len() - 4..])
        } else {
            f.write_str("****")
        }
    }
}

/// Full session record as stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub id: SessionId,
    pub token: SessionToken,
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

    #[test]
    fn session_id_generates_unique() {
        let a = SessionId::new();
        let b = SessionId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn session_id_display() {
        let id = SessionId::new();
        let s = id.to_string();
        assert!(!s.is_empty());
        assert_eq!(s.len(), 26); // ULID is 26 chars
    }

    #[test]
    fn session_id_as_str() {
        let id = SessionId::new();
        assert!(!id.as_str().is_empty());
        assert_eq!(id.as_str().len(), 26); // ULID
    }

    #[test]
    fn session_token_length_and_hex() {
        let token = SessionToken::generate();
        assert_eq!(token.as_str().len(), 64, "token should be 64 hex chars");
        assert!(
            token.as_str().chars().all(|c| c.is_ascii_hexdigit()),
            "token should contain only hex chars"
        );
    }

    #[test]
    fn session_token_unique() {
        let a = SessionToken::generate();
        let b = SessionToken::generate();
        assert_ne!(a, b, "two generated tokens should differ");
    }
}
