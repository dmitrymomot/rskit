use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

use super::backend::EmbeddingBackend;
use super::config::MistralConfig;
use super::convert::to_f32_blob;

/// Fixed output dimensions for `mistral-embed`. The Mistral API does not
/// accept a `dimensions` parameter — all models return 1024-dimensional
/// vectors.
const DIMENSIONS: usize = 1024;

struct Inner {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

/// Mistral embedding provider.
///
/// Calls `POST https://api.mistral.ai/v1/embeddings` and returns a
/// little-endian f32 blob.
///
/// # Example
///
/// ```rust,ignore
/// let client = reqwest::Client::new();
/// let provider = MistralEmbedding::new(client, &config)?;
/// let embedder = EmbeddingProvider::new(provider);
/// ```
pub struct MistralEmbedding(Arc<Inner>);

impl Clone for MistralEmbedding {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl MistralEmbedding {
    /// Create from config. Validates config at construction.
    ///
    /// # Errors
    ///
    /// Returns `Error::bad_request` if config validation fails.
    pub fn new(client: reqwest::Client, config: &MistralConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self(Arc::new(Inner {
            client,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
        })))
    }
}

impl EmbeddingBackend for MistralEmbedding {
    fn embed(&self, input: &str) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>> {
        let input = input.to_owned();
        Box::pin(async move {
            const URL: &str = concat!("https://api.mistral.ai", "/v1/embeddings");
            let body = Request {
                input: &input,
                model: &self.0.model,
            };

            let resp = self
                .0
                .client
                .post(URL)
                .bearer_auth(&self.0.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    Error::internal(format!("mistral embeddings request failed: {e}")).chain(e)
                })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(Error::internal(format!(
                    "mistral embedding error: {status}: {text}"
                )));
            }

            let parsed: Response = resp.json().await.map_err(|e| {
                Error::internal("failed to parse mistral embedding response").chain(e)
            })?;

            let values = parsed
                .data
                .into_iter()
                .next()
                .ok_or_else(|| Error::internal("mistral returned empty embedding data"))?
                .embedding;

            Ok(to_f32_blob(&values))
        })
    }

    fn dimensions(&self) -> usize {
        DIMENSIONS
    }

    fn model_name(&self) -> &str {
        &self.0.model
    }
}

#[derive(Serialize)]
struct Request<'a> {
    input: &'a str,
    model: &'a str,
}

#[derive(Deserialize)]
struct Response {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}
