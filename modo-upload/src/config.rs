use serde::Deserialize;

/// Storage backend selector.
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    /// Local filesystem storage (default).
    #[default]
    Local,
    /// S3-compatible object storage (requires the `opendal` feature).
    S3,
}

/// Upload configuration, deserialized from YAML via `modo::config::load()`.
///
/// The `s3` field is only available when the `opendal` feature is enabled.
/// Irrelevant fields are silently ignored for the active backend.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UploadConfig {
    /// Which storage backend to use.
    pub backend: StorageBackend,
    /// Local directory for file uploads.
    pub path: String,
    /// Default max file size when no per-field `#[upload(max_size)]` is set.
    /// Human-readable: "10mb", "500kb". None disables the default limit.
    pub max_file_size: Option<String>,
    /// S3 configuration (only available with the `opendal` feature).
    #[cfg(feature = "opendal")]
    pub s3: S3Config,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::default(),
            path: "./uploads".to_string(),
            max_file_size: Some("10mb".to_string()),
            #[cfg(feature = "opendal")]
            s3: S3Config::default(),
        }
    }
}

impl UploadConfig {
    /// Validate configuration at startup. Panics if `max_file_size` is set but
    /// parses to zero or is not a valid size string.
    ///
    /// Call this during application startup (e.g., in the storage factory) to
    /// fail fast rather than discovering bad config at request time.
    pub fn validate(&self) {
        if let Some(ref size_str) = self.max_file_size {
            let bytes = modo::config::parse_size(size_str).unwrap_or_else(|e| {
                panic!("invalid max_file_size \"{size_str}\": {e}");
            });
            assert!(
                bytes > 0,
                "max_file_size must be greater than 0, got \"{size_str}\""
            );
        }
    }
}

/// S3-compatible storage configuration.
#[cfg(feature = "opendal")]
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct S3Config {
    /// S3 bucket name.
    pub bucket: String,
    /// AWS region.
    pub region: String,
    /// Custom endpoint URL (for S3-compatible services like MinIO).
    pub endpoint: String,
    /// AWS access key ID.
    pub access_key_id: String,
    /// AWS secret access key.
    pub secret_access_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        // Should not panic
        let config = UploadConfig::default();
        config.validate();
    }

    #[test]
    #[should_panic(expected = "max_file_size")]
    fn rejects_zero_max_file_size() {
        let config = UploadConfig {
            max_file_size: Some("0".to_string()),
            ..Default::default()
        };
        config.validate();
    }

    #[test]
    #[should_panic(expected = "max_file_size")]
    fn rejects_zero_bytes_max_file_size() {
        let config = UploadConfig {
            max_file_size: Some("0mb".to_string()),
            ..Default::default()
        };
        config.validate();
    }

    #[test]
    #[should_panic(expected = "max_file_size")]
    fn rejects_unparseable_max_file_size() {
        let config = UploadConfig {
            max_file_size: Some("not-a-size".to_string()),
            ..Default::default()
        };
        config.validate();
    }

    #[test]
    fn none_max_file_size_is_valid() {
        let config = UploadConfig {
            max_file_size: None,
            ..Default::default()
        };
        config.validate();
    }
}
