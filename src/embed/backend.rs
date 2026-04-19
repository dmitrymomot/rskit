use std::pin::Pin;

use crate::error::Result;

/// Trait for embedding providers.
///
/// Implementations call an external embedding API and return the result as a
/// little-endian f32 byte blob ready for libsql `F32_BLOB` columns.
///
/// The built-in providers are [`super::OpenAIEmbedding`],
/// [`super::GeminiEmbedding`], [`super::MistralEmbedding`], and
/// [`super::VoyageEmbedding`]. Custom providers implement this trait directly.
pub trait EmbeddingBackend: Send + Sync {
    /// Embed a single text string.
    ///
    /// Returns a little-endian f32 byte blob of length `dimensions() * 4`.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying provider API call fails, the
    /// response cannot be parsed, or the provider returns no embedding data.
    fn embed(&self, input: &str) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>>;

    /// Number of dimensions this provider/model produces.
    fn dimensions(&self) -> usize;

    /// Model identifier string (e.g. `"text-embedding-3-small"`).
    fn model_name(&self) -> &str;
}
