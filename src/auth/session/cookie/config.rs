use serde::Deserialize;

fn deserialize_nonzero_usize<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = usize::deserialize(deserializer)?;
    if value == 0 {
        return Err(serde::de::Error::custom(
            "max_sessions_per_user must be > 0; setting it to 0 would lock out all users",
        ));
    }
    Ok(value)
}

/// Configuration for the session middleware.
///
/// Deserialised from the `session` key in the application YAML config.
/// All fields have defaults, so an empty `session:` block is valid.
///
/// # YAML example
///
/// ```yaml
/// session:
///   session_ttl_secs: 2592000   # 30 days
///   cookie_name: "_session"
///   validate_fingerprint: true
///   touch_interval_secs: 300    # 5 minutes
///   max_sessions_per_user: 10
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    /// Session lifetime in seconds. Defaults to `2_592_000` (30 days).
    pub session_ttl_secs: u64,
    /// Name of the session cookie. Defaults to `"_session"`.
    pub cookie_name: String,
    /// When `true`, the middleware rejects requests whose browser fingerprint
    /// does not match the one recorded at login. Defaults to `true`.
    pub validate_fingerprint: bool,
    /// Minimum interval between `last_active_at` updates, in seconds.
    /// A session is only touched when at least this many seconds have elapsed
    /// since the last touch. Defaults to `300` (5 minutes).
    pub touch_interval_secs: u64,
    /// Maximum number of concurrent active sessions per user. When exceeded,
    /// the least-recently-used session is evicted. Must be greater than zero.
    /// Defaults to `10`.
    #[serde(deserialize_with = "deserialize_nonzero_usize")]
    pub max_sessions_per_user: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            session_ttl_secs: 2_592_000,
            cookie_name: "_session".to_string(),
            validate_fingerprint: true,
            touch_interval_secs: 300,
            max_sessions_per_user: 10,
        }
    }
}
