//! # modo::embed
//!
//! Text-to-vector embeddings via LLM provider APIs.
//!
//! Requires feature `"text-embedding"` (depends on `"http-client"`).
//!
//! ```toml
//! [dependencies]
//! modo = { version = "0.5", features = ["text-embedding"] }
//! ```
//!
//! Provides:
//!
//! - [`EmbeddingProvider`] — concrete wrapper for any embedding backend
//! - [`EmbeddingBackend`] — trait for custom embedding providers
//! - [`OpenAIEmbedding`] — OpenAI embedding provider
//! - [`GeminiEmbedding`] — Google Gemini embedding provider
//! - [`MistralEmbedding`] — Mistral embedding provider
//! - [`VoyageEmbedding`] — Voyage AI embedding provider
//! - [`OpenAIConfig`] / [`GeminiConfig`] / [`MistralConfig`] / [`VoyageConfig`] — provider configs
//! - [`to_f32_blob`] / [`from_f32_blob`] — vector ↔ blob conversion helpers
//! - [`test::InMemoryBackend`] — in-memory backend for unit tests (`#[cfg(test)]` or `test-helpers`)
//!
//! ## Quick start
//!
//! ```rust,ignore
//! use modo::embed::{EmbeddingProvider, OpenAIEmbedding, OpenAIConfig};
//!
//! let config = OpenAIConfig {
//!     api_key: "sk-...".into(),
//!     ..Default::default()
//! };
//! let embedder = EmbeddingProvider::new(
//!     OpenAIEmbedding::new(http_client, &config)?,
//! );
//!
//! let blob = embedder.embed("hello world").await?;
//! // Store blob in libsql F32_BLOB column
//! ```

mod backend;
mod config;
mod convert;
mod gemini;
mod mistral;
mod openai;
mod provider;
mod voyage;

pub use backend::EmbeddingBackend;
pub use config::{GeminiConfig, MistralConfig, OpenAIConfig, VoyageConfig};
pub use convert::{from_f32_blob, to_f32_blob};
pub use gemini::GeminiEmbedding;
pub use mistral::MistralEmbedding;
pub use openai::OpenAIEmbedding;
pub use provider::EmbeddingProvider;
pub use voyage::VoyageEmbedding;

/// Test helpers for the embedding module.
///
/// Available when running tests or when the `test-helpers` feature is enabled.
#[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]
pub mod test {
    mod test_helpers;
    pub use test_helpers::InMemoryBackend;
}
