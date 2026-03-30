use serde::Deserialize;

use crate::error::{Error, Result};

fn default_prefix() -> String {
    "modo".into()
}

fn default_secret_length() -> usize {
    32
}

fn default_touch_threshold_secs() -> u64 {
    60
}

/// Configuration for the API key module.
///
/// Deserialised from the `apikey` key in the application YAML config.
/// All fields have defaults, so an empty `apikey:` block is valid.
///
/// # YAML example
///
/// ```yaml
/// apikey:
///   prefix: "modo"
///   secret_length: 32
///   touch_threshold_secs: 60
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ApiKeyConfig {
    /// Key prefix prepended before the underscore separator.
    /// Must be `[a-zA-Z0-9]`, 1-20 characters.
    /// Defaults to `"modo"`.
    #[serde(default = "default_prefix")]
    pub prefix: String,
    /// Length of the random secret portion in base62 characters.
    /// Minimum 16. Defaults to `32`.
    #[serde(default = "default_secret_length")]
    pub secret_length: usize,
    /// Minimum interval between `last_used_at` updates, in seconds.
    /// Defaults to `60` (1 minute).
    #[serde(default = "default_touch_threshold_secs")]
    pub touch_threshold_secs: u64,
}

impl Default for ApiKeyConfig {
    fn default() -> Self {
        Self {
            prefix: "modo".into(),
            secret_length: 32,
            touch_threshold_secs: 60,
        }
    }
}

impl ApiKeyConfig {
    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns `Error::bad_request` if the prefix is invalid or secret length
    /// is too short.
    pub fn validate(&self) -> Result<()> {
        if self.prefix.is_empty() || self.prefix.len() > 20 {
            return Err(Error::bad_request(
                "apikey prefix must be 1-20 characters",
            ));
        }
        if !self.prefix.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(Error::bad_request(
                "apikey prefix must contain only ASCII alphanumeric characters",
            ));
        }
        if self.secret_length < 16 {
            return Err(Error::bad_request(
                "apikey secret_length must be at least 16",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let config = ApiKeyConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn reject_empty_prefix() {
        let mut config = ApiKeyConfig::default();
        config.prefix = "".into();
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn reject_prefix_over_20_chars() {
        let mut config = ApiKeyConfig::default();
        config.prefix = "a".repeat(21);
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn reject_prefix_with_underscore() {
        let mut config = ApiKeyConfig::default();
        config.prefix = "my_prefix".into();
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn reject_prefix_with_special_chars() {
        let mut config = ApiKeyConfig::default();
        config.prefix = "my-prefix".into();
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn reject_short_secret_length() {
        let mut config = ApiKeyConfig::default();
        config.secret_length = 15;
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn accept_minimum_secret_length() {
        let mut config = ApiKeyConfig::default();
        config.secret_length = 16;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn deserialize_from_yaml() {
        let yaml = r#"
prefix: "sk"
secret_length: 48
touch_threshold_secs: 120
"#;
        let config: ApiKeyConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.prefix, "sk");
        assert_eq!(config.secret_length, 48);
        assert_eq!(config.touch_threshold_secs, 120);
    }

    #[test]
    fn defaults_applied_when_fields_omitted() {
        let yaml = "{}";
        let config: ApiKeyConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.prefix, "modo");
        assert_eq!(config.secret_length, 32);
        assert_eq!(config.touch_threshold_secs, 60);
    }
}
