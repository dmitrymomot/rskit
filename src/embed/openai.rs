use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

use super::backend::EmbeddingBackend;
use super::config::OpenAIConfig;
use super::convert::to_f32_blob;

const DEFAULT_BASE_URL: &str = "https://api.openai.com";

struct Inner {
    client: reqwest::Client,
    api_key: String,
    model: String,
    dimensions: usize,
    base_url: String,
}

/// OpenAI embedding provider.
///
/// Calls `POST {base_url}/v1/embeddings` and returns a little-endian f32 blob.
///
/// # Example
///
/// ```rust,ignore
/// let client = reqwest::Client::new();
/// let provider = OpenAIEmbedding::new(client, &config)?;
/// let embedder = EmbeddingProvider::new(provider);
/// ```
pub struct OpenAIEmbedding(Arc<Inner>);

impl Clone for OpenAIEmbedding {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl OpenAIEmbedding {
    /// Create from config. Validates config at construction.
    ///
    /// # Errors
    ///
    /// Returns `Error::bad_request` if config validation fails.
    pub fn new(client: reqwest::Client, config: &OpenAIConfig) -> Result<Self> {
        config.validate()?;
        let base_url = config
            .base_url
            .as_deref()
            .unwrap_or(DEFAULT_BASE_URL)
            .trim_end_matches('/')
            .to_owned();
        Ok(Self(Arc::new(Inner {
            client,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            dimensions: config.dimensions,
            base_url,
        })))
    }
}

impl EmbeddingBackend for OpenAIEmbedding {
    fn embed(&self, input: &str) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>> {
        let input = input.to_owned();
        Box::pin(async move {
            let url = format!("{}/v1/embeddings", self.0.base_url);
            let body = Request {
                input: &input,
                model: &self.0.model,
                dimensions: self.0.dimensions,
            };

            let resp = self
                .0
                .client
                .post(&url)
                .bearer_auth(&self.0.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| Error::internal("openai embeddings request failed").chain(e))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(Error::internal(format!(
                    "openai embedding error: {status}: {text}"
                )));
            }

            let parsed: Response = resp.json().await.map_err(|e| {
                Error::internal("failed to parse openai embedding response").chain(e)
            })?;

            let values = parsed
                .data
                .into_iter()
                .next()
                .ok_or_else(|| Error::internal("openai returned empty embedding data"))?
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
    dimensions: usize,
}

#[derive(Deserialize)]
struct Response {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}
