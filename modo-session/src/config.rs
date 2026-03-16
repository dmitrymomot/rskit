use serde::Deserialize;

/// Configuration for the session subsystem.
///
/// All fields have sane defaults (see [`Default`]). Deserialises from YAML/TOML
/// with `#[serde(default)]`, so you only need to specify the fields you want to
/// override.
///
/// # Example (YAML)
///
/// ```yaml
/// session_ttl_secs: 86400
/// cookie_name: "_sess"
/// validate_fingerprint: true
/// touch_interval_secs: 600
/// max_sessions_per_user: 5
/// trusted_proxies:
///   - "10.0.0.0/8"
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    /// Session lifetime in seconds (default: 2 592 000 = 30 days).
    pub session_ttl_secs: u64,
    /// Name of the HTTP cookie that carries the session token (default: `"_session"`).
    pub cookie_name: String,
    /// Whether to reject sessions whose request fingerprint changed since creation
    /// (default: `true`).  Disabling this reduces hijack protection but allows users
    /// behind rotating IPs or proxies to keep their session.
    pub validate_fingerprint: bool,
    /// Minimum number of seconds between consecutive `touch` (expiry renewal) DB
    /// writes (default: 300 = 5 minutes).
    pub touch_interval_secs: u64,
    /// Maximum number of concurrent active sessions per user before the
    /// least-recently-used session is evicted (default: 10).
    pub max_sessions_per_user: usize,
    /// CIDR ranges of trusted reverse-proxy addresses.
    ///
    /// When non-empty, the `X-Forwarded-For` / `X-Real-IP` headers are only
    /// trusted when the TCP connection originates from one of these ranges.
    ///
    /// **Security:** When empty (the default), proxy headers are trusted
    /// unconditionally — any client can spoof their IP. In production behind a
    /// reverse proxy, always set this to your proxy's CIDR range. Without a
    /// reverse proxy, set a dummy value like `["127.0.0.1/32"]` to disable
    /// proxy header trust entirely.
    pub trusted_proxies: Vec<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            session_ttl_secs: 2_592_000, // 30 days
            cookie_name: "_session".to_string(),
            validate_fingerprint: true,
            touch_interval_secs: 300, // 5 minutes
            max_sessions_per_user: 10,
            trusted_proxies: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let config = SessionConfig::default();
        assert_eq!(config.session_ttl_secs, 2_592_000);
        assert_eq!(config.cookie_name, "_session");
        assert!(config.validate_fingerprint);
        assert_eq!(config.touch_interval_secs, 300);
        assert_eq!(config.max_sessions_per_user, 10);
        assert!(config.trusted_proxies.is_empty());
    }

    #[test]
    fn partial_yaml_deserialization() {
        let yaml = r#"
session_ttl_secs: 3600
cookie_name: "my_sess"
"#;
        let config: SessionConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.session_ttl_secs, 3600);
        assert_eq!(config.cookie_name, "my_sess");
        // defaults for omitted fields
        assert!(config.validate_fingerprint);
        assert_eq!(config.touch_interval_secs, 300);
        assert_eq!(config.max_sessions_per_user, 10);
    }
}
