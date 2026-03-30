use serde::Deserialize;

use crate::error::{Error, Result};

fn default_openai_model() -> String {
    "text-embedding-3-small".into()
}

fn default_openai_dimensions() -> usize {
    1536
}

/// Configuration for the OpenAI embedding provider.
///
/// # YAML example
///
/// ```yaml
/// api_key: "${OPENAI_API_KEY}"
/// model: "text-embedding-3-small"
/// dimensions: 1536
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct OpenAIConfig {
    /// OpenAI API key. Required.
    pub api_key: String,
    /// Model name. Defaults to `"text-embedding-3-small"`.
    #[serde(default = "default_openai_model")]
    pub model: String,
    /// Output vector dimensions. Defaults to `1536`.
    #[serde(default = "default_openai_dimensions")]
    pub dimensions: usize,
    /// Base URL override for Azure OpenAI or compatible proxies.
    /// Defaults to `None` (uses `https://api.openai.com`).
    pub base_url: Option<String>,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "text-embedding-3-small".into(),
            dimensions: 1536,
            base_url: None,
        }
    }
}

impl OpenAIConfig {
    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns `Error::bad_request` if `api_key` is empty, `model` is empty,
    /// or `dimensions` is zero.
    pub fn validate(&self) -> Result<()> {
        if self.api_key.is_empty() {
            return Err(Error::bad_request("openai api_key must not be empty"));
        }
        if self.model.is_empty() {
            return Err(Error::bad_request("openai model must not be empty"));
        }
        if self.dimensions == 0 {
            return Err(Error::bad_request(
                "openai dimensions must be greater than 0",
            ));
        }
        Ok(())
    }
}

fn default_gemini_model() -> String {
    "gemini-embedding-001".into()
}

fn default_gemini_dimensions() -> usize {
    768
}

/// Configuration for the Gemini embedding provider.
///
/// # YAML example
///
/// ```yaml
/// api_key: "${GEMINI_API_KEY}"
/// model: "gemini-embedding-001"
/// dimensions: 768
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GeminiConfig {
    /// Gemini API key. Required.
    pub api_key: String,
    /// Model name. Defaults to `"gemini-embedding-001"`.
    #[serde(default = "default_gemini_model")]
    pub model: String,
    /// Output vector dimensions. Defaults to `768`.
    #[serde(default = "default_gemini_dimensions")]
    pub dimensions: usize,
}

impl Default for GeminiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gemini-embedding-001".into(),
            dimensions: 768,
        }
    }
}

impl GeminiConfig {
    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns `Error::bad_request` if `api_key` is empty, `model` is empty,
    /// or `dimensions` is zero.
    pub fn validate(&self) -> Result<()> {
        if self.api_key.is_empty() {
            return Err(Error::bad_request("gemini api_key must not be empty"));
        }
        if self.model.is_empty() {
            return Err(Error::bad_request("gemini model must not be empty"));
        }
        if self.dimensions == 0 {
            return Err(Error::bad_request(
                "gemini dimensions must be greater than 0",
            ));
        }
        Ok(())
    }
}

fn default_mistral_model() -> String {
    "mistral-embed".into()
}

/// Configuration for the Mistral embedding provider.
///
/// The Mistral API does not accept a `dimensions` parameter — `mistral-embed`
/// always returns 1024-dimensional vectors. Use
/// [`EmbeddingBackend::dimensions()`](super::EmbeddingBackend::dimensions) on
/// a `MistralEmbedding` to query the fixed output size.
///
/// # YAML example
///
/// ```yaml
/// api_key: "${MISTRAL_API_KEY}"
/// model: "mistral-embed"
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MistralConfig {
    /// Mistral API key. Required.
    pub api_key: String,
    /// Model name. Defaults to `"mistral-embed"`.
    #[serde(default = "default_mistral_model")]
    pub model: String,
}

impl Default for MistralConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "mistral-embed".into(),
        }
    }
}

impl MistralConfig {
    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns `Error::bad_request` if `api_key` is empty or `model` is empty.
    pub fn validate(&self) -> Result<()> {
        if self.api_key.is_empty() {
            return Err(Error::bad_request("mistral api_key must not be empty"));
        }
        if self.model.is_empty() {
            return Err(Error::bad_request("mistral model must not be empty"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- OpenAI ---

    #[test]
    fn openai_default_is_invalid_without_key() {
        let config = OpenAIConfig::default();
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn openai_valid_config() {
        let config = OpenAIConfig {
            api_key: "sk-test".into(),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn openai_reject_empty_model() {
        let config = OpenAIConfig {
            api_key: "sk-test".into(),
            model: "".into(),
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn openai_reject_zero_dimensions() {
        let config = OpenAIConfig {
            api_key: "sk-test".into(),
            dimensions: 0,
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn openai_deserialize_defaults() {
        let yaml = r#"api_key: "sk-test""#;
        let config: OpenAIConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.model, "text-embedding-3-small");
        assert_eq!(config.dimensions, 1536);
        assert!(config.base_url.is_none());
    }

    // --- Gemini ---

    #[test]
    fn gemini_default_is_invalid_without_key() {
        let config = GeminiConfig::default();
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn gemini_valid_config() {
        let config = GeminiConfig {
            api_key: "AIza-test".into(),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn gemini_reject_zero_dimensions() {
        let config = GeminiConfig {
            api_key: "AIza-test".into(),
            dimensions: 0,
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn gemini_deserialize_defaults() {
        let yaml = r#"api_key: "AIza-test""#;
        let config: GeminiConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.model, "gemini-embedding-001");
        assert_eq!(config.dimensions, 768);
    }

    // --- Mistral ---

    #[test]
    fn mistral_default_is_invalid_without_key() {
        let config = MistralConfig::default();
        let err = config.validate().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn mistral_valid_config() {
        let config = MistralConfig {
            api_key: "ms-test".into(),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn mistral_deserialize_defaults() {
        let yaml = r#"api_key: "ms-test""#;
        let config: MistralConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.model, "mistral-embed");
    }
}
