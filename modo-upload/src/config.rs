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
