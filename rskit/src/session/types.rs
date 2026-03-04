use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Opaque session identifier (ULID string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

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

impl From<String> for SessionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Full session record as stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub id: SessionId,
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
    fn session_id_from_string() {
        let id = SessionId::from("test123".to_string());
        assert_eq!(id.as_str(), "test123");
    }
}
