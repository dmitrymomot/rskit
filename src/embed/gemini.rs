use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::http;

use super::backend::EmbeddingBackend;
use super::config::GeminiConfig;
use super::convert::to_f32_blob;

const BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

struct Inner {
    client: http::Client,
    api_key: String,
    model: String,
    dimensions: usize,
}

/// Google Gemini embedding provider.
///
/// Calls the Gemini `embedContent` API and returns a little-endian f32 blob.
///
/// # Example
///
/// ```rust,ignore
/// let provider = GeminiEmbedding::new(http_client, &config)?;
/// let embedder = EmbeddingProvider::new(provider);
/// ```
pub struct GeminiEmbedding(Arc<Inner>);

impl Clone for GeminiEmbedding {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl GeminiEmbedding {
    /// Create from config. Validates config at construction.
    ///
    /// # Errors
    ///
    /// Returns `Error::bad_request` if config validation fails.
    pub fn new(client: http::Client, config: &GeminiConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self(Arc::new(Inner {
            client,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            dimensions: config.dimensions,
        })))
    }
}

impl EmbeddingBackend for GeminiEmbedding {
    fn embed(&self, input: &str) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + '_>> {
        let input = input.to_owned();
        Box::pin(async move {
            let url = format!(
                "{BASE_URL}/models/{}:embedContent?key={}",
                self.0.model, self.0.api_key,
            );
            let body = OwnedRequest {
                content: OwnedContent {
                    parts: vec![OwnedPart { text: input }],
                },
                output_dimensionality: self.0.dimensions,
            };

            let resp = self.0.client.post(&url).json(&body).send().await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(Error::internal(format!(
                    "gemini embedding error: {status}: {text}"
                )));
            }

            let parsed: Response = resp.json().await.map_err(|e| {
                Error::internal("failed to parse gemini embedding response").chain(e)
            })?;

            Ok(to_f32_blob(&parsed.embedding.values))
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
#[serde(rename_all = "camelCase")]
struct OwnedRequest {
    content: OwnedContent,
    output_dimensionality: usize,
}

#[derive(Serialize)]
struct OwnedContent {
    parts: Vec<OwnedPart>,
}

#[derive(Serialize)]
struct OwnedPart {
    text: String,
}

#[derive(Deserialize)]
struct Response {
    embedding: EmbeddingValues,
}

#[derive(Deserialize)]
struct EmbeddingValues {
    values: Vec<f32>,
}
