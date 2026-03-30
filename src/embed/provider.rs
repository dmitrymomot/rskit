use std::sync::Arc;

use crate::error::{Error, Result};

use super::backend::EmbeddingBackend;

/// Concrete embedding provider — wraps any [`EmbeddingBackend`].
///
/// Cheap to clone (wraps `Arc` internally). Use as an axum service via
/// `Service(embedder): Service<EmbeddingProvider>`.
///
/// # Example
///
/// ```rust,ignore
/// let embedder = EmbeddingProvider::new(
///     OpenAIEmbedding::new(http_client, &config)?,
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
