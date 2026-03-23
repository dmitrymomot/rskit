use std::fmt;
use std::str::FromStr;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const PREFIX: &str = "whsec_";

pub struct WebhookSecret {
    key: Vec<u8>,
}

impl WebhookSecret {
    /// Construct from raw bytes.
    pub fn new(raw: impl Into<Vec<u8>>) -> Self {
        Self { key: raw.into() }
    }

    /// Generate a new secret with 24 random bytes.
    pub fn generate() -> Self {
        let mut key = vec![0u8; 24];
        rand::fill(&mut key[..]);
        Self { key }
    }

    /// Access raw key bytes for HMAC operations.
    pub fn as_bytes(&self) -> &[u8] {
        &self.key
    }
}

impl FromStr for WebhookSecret {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let encoded = s
            .strip_prefix(PREFIX)
            .ok_or_else(|| Error::bad_request("webhook secret must start with 'whsec_'"))?;
        let key = BASE64
            .decode(encoded)
            .map_err(|e| Error::bad_request(format!("invalid base64 in webhook secret: {e}")))?;
        Ok(Self { key })
    }
}

impl fmt::Display for WebhookSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", PREFIX, BASE64.encode(&self.key))
    }
}

impl fmt::Debug for WebhookSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WebhookSecret(***)")
    }
}

impl Serialize for WebhookSecret {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for WebhookSecret {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_whsec_string() {
        let raw = vec![1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let encoded = format!("whsec_{}", BASE64.encode(&raw));
        let secret: WebhookSecret = encoded.parse().unwrap();
        assert_eq!(secret.as_bytes(), &raw);
    }

    #[test]
    fn reject_missing_prefix() {
        let result = "notwhsec_AQIDBA==".parse::<WebhookSecret>();
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("whsec_"));
    }

    #[test]
    fn reject_invalid_base64() {
        let result = "whsec_!!!invalid!!!".parse::<WebhookSecret>();
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("base64"));
    }

    #[test]
    fn display_roundtrip() {
        let secret = WebhookSecret::new(vec![10, 20, 30, 40]);
        let displayed = secret.to_string();
        assert!(displayed.starts_with("whsec_"));
        let parsed: WebhookSecret = displayed.parse().unwrap();
        assert_eq!(parsed.as_bytes(), secret.as_bytes());
    }

    #[test]
    fn debug_is_redacted() {
        let secret = WebhookSecret::new(vec![1, 2, 3]);
        let debug = format!("{secret:?}");
        assert_eq!(debug, "WebhookSecret(***)");
        assert!(!debug.contains("1"));
    }

    #[test]
    fn generate_produces_valid_secret() {
        let secret = WebhookSecret::generate();
        assert_eq!(secret.as_bytes().len(), 24);
        // Round-trip through Display/FromStr
        let displayed = secret.to_string();
        let parsed: WebhookSecret = displayed.parse().unwrap();
        assert_eq!(parsed.as_bytes(), secret.as_bytes());
    }

    #[test]
    fn serialize_roundtrip() {
        let secret = WebhookSecret::new(vec![5, 10, 15, 20]);
        let json = serde_json::to_string(&secret).unwrap();
        let parsed: WebhookSecret = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.as_bytes(), secret.as_bytes());
    }

    #[test]
    fn deserialize_from_string() {
        let raw = vec![99u8; 16];
        let whsec = format!("\"whsec_{}\"", BASE64.encode(&raw));
        let secret: WebhookSecret = serde_json::from_str(&whsec).unwrap();
        assert_eq!(secret.as_bytes(), &raw);
    }
}
