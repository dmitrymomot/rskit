use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

use super::backend::EmbeddingBackend;
use super::config::VoyageConfig;
use super::convert::to_f32_blob;

struct Inner {
    client: reqwest::Client,
    api_key: String,
    model: String,
    dimensions: usize,
}

/// Voyage AI embedding provider.
///
/// Calls `POST https://api.voyageai.com/v1/embeddings` and returns a
/// little-endian f32 blob.
///
/// # Example
///
/// ```rust,ignore
/// let client = reqwest::Client::new();
/// let provider = VoyageEmbedding::new(client, &config)?;
/// let embedder = EmbeddingProvider::new(provider);
/// ```
pub struct VoyageEmbedding(Arc<Inner>);

impl Clone for VoyageEmbedding {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl VoyageEmbedding {
    /// Create from config. Validates config at construction.
    ///
    /// # Errors
    ///
    /// Returns `Error::bad_request` if config validation fails.
    pub fn new(client: reqwest::Client, config: &VoyageConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self(Arc::new(Inner {
            client,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            dimensions: config.dimensions,
        })))
    }
}

impl EmbeddingBackend for VoyageEmbedding {
    fn embed(&self, input: &str) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>> {
        let input = input.to_owned();
        Box::pin(async move {
            const URL: &str = concat!("https://api.voyageai.com", "/v1/embeddings");
            let body = Request {
                input: &input,
                model: &self.0.model,
                output_dimension: self.0.dimensions,
            };

            let resp = self
                .0
                .client
                .post(URL)
                .bearer_auth(&self.0.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| Error::internal("voyage embeddings request failed").chain(e))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(Error::internal(format!(
                    "voyage embedding error: {status}: {text}"
                )));
            }

            let parsed: Response = resp.json().await.map_err(|e| {
                Error::internal("failed to parse voyage embedding response").chain(e)
            })?;

            let values = parsed
                .data
                .into_iter()
                .next()
                .ok_or_else(|| Error::internal("voyage returned empty embedding data"))?
                .embedding;

            Ok(to_f32_blob(&values))
        })
    }

    fn dimensions(&self) -> usize {
        self.0.dimensions
    }

    fn model_name(&self) -> &str {
        &self.0.model
    }
}

#[derive(Serialize)]
struct Request<'a> {
    input: &'a str,
    model: &'a str,
    output_dimension: usize,
}

#[derive(Deserialize)]
struct Response {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}
