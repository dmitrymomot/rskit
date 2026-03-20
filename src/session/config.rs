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

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    pub session_ttl_secs: u64,
    pub cookie_name: String,
    pub validate_fingerprint: bool,
    pub touch_interval_secs: u64,
    #[serde(deserialize_with = "deserialize_nonzero_usize")]
    pub max_sessions_per_user: usize,
    pub trusted_proxies: Vec<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            session_ttl_secs: 2_592_000,
            cookie_name: "_session".to_string(),
            validate_fingerprint: true,
            touch_interval_secs: 300,
            max_sessions_per_user: 10,
            trusted_proxies: Vec::new(),
        }
    }
}
