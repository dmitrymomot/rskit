use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{Error, Result};

use super::config::BucketConfig;
use super::storage::Storage;

/// Named collection of `Storage` instances for multi-bucket apps.
///
/// Cheaply cloneable (wraps `Arc`). Each entry is a `Storage` keyed by name.
pub struct Buckets {
    inner: Arc<HashMap<String, Storage>>,
}

impl Clone for Buckets {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Buckets {
    /// Create from a list of bucket configs.
    ///
    /// Each config must have a unique `name`. Returns an error on duplicates
    /// or invalid config.
    pub fn new(configs: &[BucketConfig]) -> Result<Self> {
        let mut map = HashMap::with_capacity(configs.len());
        for config in configs {
            if config.name.is_empty() {
                return Err(Error::internal(
                    "bucket config must have a name when used with Buckets",
                ));
            }
            if map.contains_key(&config.name) {
                return Err(Error::internal(format!(
                    "duplicate bucket name '{}'",
                    config.name
                )));
            }
            let storage = Storage::new(config)?;
            map.insert(config.name.clone(), storage);
        }
        Ok(Self {
            inner: Arc::new(map),
        })
    }

    /// Get a `Storage` by name (cloned — cheap `Arc` clone).
    ///
    /// Returns an error if no bucket with that name is configured.
    pub fn get(&self, name: &str) -> Result<Storage> {
        self.inner
            .get(name)
            .cloned()
            .ok_or_else(|| Error::internal(format!("bucket '{name}' not configured")))
    }

    /// Create named in-memory buckets for testing.
    #[cfg(any(test, feature = "upload-test"))]
    pub fn memory(names: &[&str]) -> Self {
        let mut map = HashMap::with_capacity(names.len());
        for name in names {
            map.insert((*name).to_string(), Storage::memory());
        }
        Self {
            inner: Arc::new(map),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::UploadedFile;
    use bytes::Bytes;

    fn test_file() -> UploadedFile {
        UploadedFile {
            name: "test.txt".to_string(),
            content_type: "text/plain".to_string(),
            size: 5,
            data: Bytes::from_static(b"hello"),
        }
    }

    #[tokio::test]
    async fn memory_buckets_get_existing() {
        let buckets = Buckets::memory(&["avatars", "docs"]);
        let store = buckets.get("avatars").unwrap();
        let file = test_file();
        let key = store.put(&file, "test/").await.unwrap();
        assert!(store.exists(&key).await.unwrap());
    }

    #[test]
    fn get_unknown_name_returns_error() {
        let buckets = Buckets::memory(&["avatars"]);
        assert!(buckets.get("nonexistent").is_err());
    }

    #[tokio::test]
    async fn buckets_are_isolated() {
        let buckets = Buckets::memory(&["a", "b"]);
        let store_a = buckets.get("a").unwrap();
        let store_b = buckets.get("b").unwrap();

        let file = test_file();
        let key = store_a.put(&file, "test/").await.unwrap();

        assert!(store_a.exists(&key).await.unwrap());
        // Different memory operator — file should not exist in b
        assert!(!store_b.exists(&key).await.unwrap());
    }

    #[test]
    fn empty_names_vec_is_valid() {
        let buckets = Buckets::memory(&[]);
        assert!(buckets.get("anything").is_err());
    }

    #[test]
    fn clone_is_cheap() {
        let buckets = Buckets::memory(&["a"]);
        let cloned = buckets.clone();
        // Both point to the same Arc
        assert!(cloned.get("a").is_ok());
    }
}
