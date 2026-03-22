use serde::Deserialize;

use crate::error::{Error, Result};

/// Configuration for a single S3-compatible storage bucket.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BucketConfig {
    /// Name used as the lookup key in `Buckets`. Ignored by `Storage::new()`.
    pub name: String,
    /// S3 bucket name.
    pub bucket: String,
    /// AWS region (e.g. `us-east-1`). `None` uses `us-east-1` by default.
    pub region: Option<String>,
    /// S3-compatible endpoint URL.
    pub endpoint: String,
    /// Access key ID.
    pub access_key: String,
    /// Secret access key.
    pub secret_key: String,
    /// Base URL for public (non-signed) file URLs. `None` means `url()` will error.
    pub public_url: Option<String>,
    /// Maximum file size in human-readable format (e.g. `"10mb"`). `None` disables the limit.
    pub max_file_size: Option<String>,
    /// Use path-style URLs (e.g. `https://endpoint/bucket/key`). Defaults to `true`.
    /// Set to `false` for virtual-hosted-style (e.g. `https://bucket.endpoint/key`).
    pub path_style: bool,
}

impl Default for BucketConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            bucket: String::new(),
            region: None,
            endpoint: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            public_url: None,
            max_file_size: None,
            path_style: true,
        }
    }
}

#[allow(dead_code)]
impl BucketConfig {
    /// Validate configuration. Returns an error if required fields are missing
    /// or `max_file_size` is invalid. Called by `Storage::new()`.
    pub(crate) fn validate(&self) -> Result<()> {
        if self.bucket.is_empty() {
            return Err(Error::internal("bucket name is required"));
        }
        if self.endpoint.is_empty() {
            return Err(Error::internal("endpoint is required"));
        }
        if let Some(ref size_str) = self.max_file_size {
            parse_size(size_str)?; // validates format and > 0
        }
        Ok(())
    }

    /// Normalize the config: trim `public_url`, convert empty to `None`.
    pub(crate) fn normalized_public_url(&self) -> Option<String> {
        self.public_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_end_matches('/').to_string())
    }

    /// Parse `max_file_size` to bytes. Returns `None` if not set.
    pub(crate) fn max_file_size_bytes(&self) -> Result<Option<usize>> {
        match &self.max_file_size {
            Some(s) => Ok(Some(parse_size(s)?)),
            None => Ok(None),
        }
    }
}

/// Parse a human-readable size string into bytes.
///
/// Format: `<number><unit>` where unit is `b`, `kb`, `mb`, `gb` (case-insensitive).
/// Bare numbers (e.g. `"1024"`) are treated as bytes.
#[allow(dead_code)]
pub(crate) fn parse_size(s: &str) -> Result<usize> {
    let s = s.trim().to_ascii_lowercase();
    if s.is_empty() {
        return Err(Error::internal("empty size string"));
    }

    let (num_str, multiplier) = if let Some(n) = s.strip_suffix("gb") {
        (n, 1024 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("mb") {
        (n, 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("kb") {
        (n, 1024)
    } else if let Some(n) = s.strip_suffix('b') {
        (n, 1)
    } else {
        (s.as_str(), 1)
    };

    let num: usize = num_str
        .trim()
        .parse()
        .map_err(|_| Error::internal(format!("invalid size string: \"{s}\"")))?;

    let result = num * multiplier;
    if result == 0 {
        return Err(Error::internal(format!(
            "size must be greater than 0: \"{s}\""
        )));
    }

    Ok(result)
}

/// Convert kilobytes to bytes.
pub fn kb(n: usize) -> usize {
    n * 1024
}

/// Convert megabytes to bytes.
pub fn mb(n: usize) -> usize {
    n * 1024 * 1024
}

/// Convert gigabytes to bytes.
pub fn gb(n: usize) -> usize {
    n * 1024 * 1024 * 1024
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_size --

    #[test]
    fn parse_size_mb() {
        assert_eq!(parse_size("10mb").unwrap(), 10 * 1024 * 1024);
    }

    #[test]
    fn parse_size_kb() {
        assert_eq!(parse_size("500kb").unwrap(), 500 * 1024);
    }

    #[test]
    fn parse_size_gb() {
        assert_eq!(parse_size("1gb").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_size_bytes_with_suffix() {
        assert_eq!(parse_size("1024b").unwrap(), 1024);
    }

    #[test]
    fn parse_size_bare_number() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
    }

    #[test]
    fn parse_size_case_insensitive() {
        assert_eq!(parse_size("10MB").unwrap(), 10 * 1024 * 1024);
        assert_eq!(parse_size("5Kb").unwrap(), 5 * 1024);
    }

    #[test]
    fn parse_size_with_whitespace() {
        assert_eq!(parse_size("  10mb  ").unwrap(), 10 * 1024 * 1024);
    }

    #[test]
    fn parse_size_empty_string() {
        assert!(parse_size("").is_err());
    }

    #[test]
    fn parse_size_invalid() {
        assert!(parse_size("abc").is_err());
        assert!(parse_size("mb").is_err());
    }

    #[test]
    fn parse_size_zero_rejected() {
        assert!(parse_size("0mb").is_err());
        assert!(parse_size("0").is_err());
    }

    // -- size helpers --

    #[test]
    fn size_helpers() {
        assert_eq!(kb(1), 1024);
        assert_eq!(mb(1), 1024 * 1024);
        assert_eq!(gb(1), 1024 * 1024 * 1024);
        assert_eq!(mb(5), 5 * 1024 * 1024);
    }

    // -- BucketConfig validation --

    #[test]
    fn valid_config() {
        let config = BucketConfig {
            bucket: "test".into(),
            endpoint: "https://s3.example.com".into(),
            ..Default::default()
        };
        config.validate().unwrap();
    }

    #[test]
    fn rejects_empty_bucket() {
        let config = BucketConfig {
            endpoint: "https://s3.example.com".into(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_empty_endpoint() {
        let config = BucketConfig {
            bucket: "test".into(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_invalid_max_file_size() {
        let config = BucketConfig {
            bucket: "test".into(),
            endpoint: "https://s3.example.com".into(),
            max_file_size: Some("not-a-size".into()),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_zero_max_file_size() {
        let config = BucketConfig {
            bucket: "test".into(),
            endpoint: "https://s3.example.com".into(),
            max_file_size: Some("0mb".into()),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn none_max_file_size_is_valid() {
        let config = BucketConfig {
            bucket: "test".into(),
            endpoint: "https://s3.example.com".into(),
            max_file_size: None,
            ..Default::default()
        };
        config.validate().unwrap();
    }

    #[test]
    fn normalized_public_url_strips_trailing_slash() {
        let config = BucketConfig {
            public_url: Some("https://cdn.example.com/".into()),
            ..Default::default()
        };
        assert_eq!(
            config.normalized_public_url(),
            Some("https://cdn.example.com".into())
        );
    }

    #[test]
    fn normalized_public_url_empty_becomes_none() {
        let config = BucketConfig {
            public_url: Some("".into()),
            ..Default::default()
        };
        assert_eq!(config.normalized_public_url(), None);
    }

    #[test]
    fn normalized_public_url_whitespace_becomes_none() {
        let config = BucketConfig {
            public_url: Some("   ".into()),
            ..Default::default()
        };
        assert_eq!(config.normalized_public_url(), None);
    }

    #[test]
    fn normalized_public_url_none_stays_none() {
        let config = BucketConfig::default();
        assert_eq!(config.normalized_public_url(), None);
    }

    #[test]
    fn default_path_style_is_true() {
        let config = BucketConfig::default();
        assert!(config.path_style);
    }

    #[test]
    fn default_region_is_none() {
        let config = BucketConfig::default();
        assert!(config.region.is_none());
    }
}
