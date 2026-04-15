use std::sync::Arc;

use crate::error::{Error, Result};

use super::backend::EmbeddingBackend;

/// Concrete embedding provider — wraps any [`EmbeddingBackend`].
///
/// Cheap to clone (wraps `Arc` internally). Use as an axum service via
/// `Service(embedder): Service<EmbeddingProvider>` where `embedder` is
/// `Arc<EmbeddingProvider>`; `Arc<T>` derefs to `T` so calling `.embed()`
/// directly on `embedder` works without extra unwrapping.
///
/// # Example
///
/// ```rust,ignore
/// let client = reqwest::Client::new();
/// let embedder = EmbeddingProvider::new(
///     OpenAIEmbedding::new(client, &config)?,
/// );
/// let blob = embedder.embed("hello world").await?;
/// ```
pub struct EmbeddingProvider(Arc<dyn EmbeddingBackend>);

impl Clone for EmbeddingProvider {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl EmbeddingProvider {
    /// Wrap any backend. `Arc` is handled internally.
    pub fn new(backend: impl EmbeddingBackend + 'static) -> Self {
        Self(Arc::new(backend))
    }

    /// Embed text. Returns a little-endian f32 blob for libsql.
    ///
    /// # Errors
    ///
    /// Returns `Error::bad_request` if `input` is empty.
    /// Propagates provider API errors.
    pub async fn embed(&self, input: &str) -> Result<Vec<u8>> {
        if input.is_empty() {
            return Err(Error::bad_request("embedding input must not be empty"));
        }
        self.0.embed(input).await
    }

    /// Number of dimensions this provider/model produces.
    pub fn dimensions(&self) -> usize {
        self.0.dimensions()
    }

    /// Model identifier string.
    pub fn model_name(&self) -> &str {
        self.0.model_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::convert::from_f32_blob;
    use crate::embed::test::InMemoryBackend;

    #[tokio::test]
    async fn embed_returns_blob_of_correct_length() {
        let dims = 128;
        let provider = EmbeddingProvider::new(InMemoryBackend::new(dims));
        let blob = provider.embed("hello").await.unwrap();
        assert_eq!(blob.len(), dims * 4);
    }

    #[tokio::test]
    async fn embed_blob_roundtrips_to_floats() {
        let dims = 4;
        let provider = EmbeddingProvider::new(InMemoryBackend::new(dims));
        let blob = provider.embed("test").await.unwrap();
        let floats = from_f32_blob(&blob).unwrap();
        assert_eq!(floats.len(), dims);
        assert_eq!(floats, vec![0.1_f32; dims]);
    }

    #[tokio::test]
    async fn embed_rejects_empty_input() {
        let provider = EmbeddingProvider::new(InMemoryBackend::new(4));
        let err = provider.embed("").await.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn dimensions_delegated() {
        let provider = EmbeddingProvider::new(InMemoryBackend::new(768));
        assert_eq!(provider.dimensions(), 768);
    }

    #[test]
    fn model_name_delegated() {
        let provider = EmbeddingProvider::new(InMemoryBackend::new(4));
        assert_eq!(provider.model_name(), "test-embedding");
    }
}
