use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use bytes::Bytes;

use super::options::{Acl, PutOptions};
use crate::error::Result;

#[allow(dead_code)]
struct StoredObject {
    data: Bytes,
    content_type: String,
    acl: Option<Acl>,
}

pub(crate) struct MemoryBackend {
    objects: RwLock<HashMap<String, StoredObject>>,
    fake_url_base: String,
}

impl MemoryBackend {
    #[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]
    pub fn new() -> Self {
        Self {
            objects: RwLock::new(HashMap::new()),
            fake_url_base: "https://memory.test".to_string(),
        }
    }

    pub async fn put(
        &self,
        key: &str,
        data: Bytes,
        content_type: &str,
        opts: &PutOptions,
    ) -> Result<()> {
        let mut map = self.objects.write().expect("lock poisoned");
        map.insert(
            key.to_string(),
            StoredObject {
                data,
                content_type: content_type.to_string(),
                acl: opts.acl,
            },
        );
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        let mut map = self.objects.write().expect("lock poisoned");
        map.remove(key);
        Ok(())
    }

    pub async fn exists(&self, key: &str) -> Result<bool> {
        let map = self.objects.read().expect("lock poisoned");
        Ok(map.contains_key(key))
    }

    pub async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let map = self.objects.read().expect("lock poisoned");
        let keys = map
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        Ok(keys)
    }

    pub async fn presigned_url(&self, key: &str, expires_in: Duration) -> Result<String> {
        Ok(format!(
            "{}/{}?expires={}",
            self.fake_url_base,
            key,
            expires_in.as_secs()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_and_exists() {
        let backend = MemoryBackend::new();
        backend
            .put(
                "test/file.txt",
                Bytes::from("hello"),
                "text/plain",
                &PutOptions::default(),
            )
            .await
            .unwrap();
        assert!(backend.exists("test/file.txt").await.unwrap());
    }

    #[tokio::test]
    async fn exists_false_for_missing() {
        let backend = MemoryBackend::new();
        assert!(!backend.exists("missing.txt").await.unwrap());
    }

    #[tokio::test]
    async fn delete_removes_key() {
        let backend = MemoryBackend::new();
        backend
            .put(
                "key.txt",
                Bytes::from("data"),
                "text/plain",
                &PutOptions::default(),
            )
            .await
            .unwrap();
        backend.delete("key.txt").await.unwrap();
        assert!(!backend.exists("key.txt").await.unwrap());
    }

    #[tokio::test]
    async fn delete_nonexistent_is_noop() {
        let backend = MemoryBackend::new();
        backend.delete("missing.txt").await.unwrap();
    }

    #[tokio::test]
    async fn list_by_prefix() {
        let backend = MemoryBackend::new();
        backend
            .put(
                "prefix/a.txt",
                Bytes::from("a"),
                "text/plain",
                &PutOptions::default(),
            )
            .await
            .unwrap();
        backend
            .put(
                "prefix/b.txt",
                Bytes::from("b"),
                "text/plain",
                &PutOptions::default(),
            )
            .await
            .unwrap();
        backend
            .put(
                "other/c.txt",
                Bytes::from("c"),
                "text/plain",
                &PutOptions::default(),
            )
            .await
            .unwrap();

        let mut keys = backend.list("prefix/").await.unwrap();
        keys.sort();
        assert_eq!(keys, vec!["prefix/a.txt", "prefix/b.txt"]);
    }

    #[tokio::test]
    async fn presigned_url_returns_fake() {
        let backend = MemoryBackend::new();
        let url = backend
            .presigned_url("test/file.txt", Duration::from_secs(3600))
            .await
            .unwrap();
        assert_eq!(url, "https://memory.test/test/file.txt?expires=3600");
    }

    #[tokio::test]
    async fn put_stores_acl() {
        let backend = MemoryBackend::new();
        let opts = PutOptions {
            acl: Some(super::super::options::Acl::PublicRead),
            ..Default::default()
        };
        backend
            .put("test/file.txt", Bytes::from("hello"), "text/plain", &opts)
            .await
            .unwrap();

        let map = backend.objects.read().expect("lock poisoned");
        let obj = map.get("test/file.txt").unwrap();
        assert_eq!(obj.acl, Some(super::super::options::Acl::PublicRead));
    }

    #[tokio::test]
    async fn put_stores_none_acl_by_default() {
        let backend = MemoryBackend::new();
        backend
            .put(
                "test/file.txt",
                Bytes::from("hello"),
                "text/plain",
                &PutOptions::default(),
            )
            .await
            .unwrap();

        let map = backend.objects.read().expect("lock poisoned");
        let obj = map.get("test/file.txt").unwrap();
        assert_eq!(obj.acl, None);
    }
}
