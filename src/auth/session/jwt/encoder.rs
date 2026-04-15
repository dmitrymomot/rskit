use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;

use crate::encoding::base64url;
use crate::{Error, Result};

use super::config::JwtSessionsConfig;
use super::error::JwtError;
use super::signer::{HmacSigner, TokenSigner};

/// JWT token encoder. Signs tokens using a [`TokenSigner`].
///
/// Register in `Registry` for handler access via `Service<JwtEncoder>`.
/// Cloning is cheap — state is stored behind `Arc`.
pub struct JwtEncoder {
    inner: Arc<JwtEncoderInner>,
}

struct JwtEncoderInner {
    signer: Arc<dyn TokenSigner>,
    default_expiry: Option<Duration>,
    validation: super::validation::ValidationConfig,
}

impl JwtEncoder {
    /// Creates a `JwtEncoder` from YAML configuration.
    ///
    /// Uses `HmacSigner` (HS256) with the configured secret. The validation
    /// config (issuer) is stored so a matching `JwtDecoder`
    /// can be created via `JwtDecoder::from(&encoder)`.
    pub fn from_config(config: &JwtSessionsConfig) -> Self {
        let signer = HmacSigner::new(config.signing_secret.as_bytes());
        Self {
            inner: Arc::new(JwtEncoderInner {
                signer: Arc::new(signer),
                default_expiry: Some(Duration::from_secs(config.access_ttl_secs)),
                validation: super::validation::ValidationConfig {
                    leeway: Duration::ZERO,
                    require_issuer: config.issuer.clone(),
                    require_audience: None,
                },
            }),
        }
    }

    /// Returns a reference to the inner signer (as verifier).
    /// Used by `JwtDecoder::from(&encoder)` to share the same key.
    pub(super) fn verifier(&self) -> Arc<dyn super::signer::TokenVerifier> {
        // Trait upcasting: Arc<dyn TokenSigner> → Arc<dyn TokenVerifier>
        // Stabilized in Rust 1.76.
        self.inner.signer.clone() as Arc<dyn super::signer::TokenVerifier>
    }

    /// Returns a clone of the validation config.
    /// Used by `JwtDecoder::from(&encoder)`.
    pub(super) fn validation(&self) -> super::validation::ValidationConfig {
        self.inner.validation.clone()
    }

    /// Encodes a serializable payload into a signed JWT token string.
    ///
    /// If the payload serializes to a JSON object without an `exp` field and
    /// `default_expiry` is configured, `exp` is automatically set to
    /// `now + default_expiry` before signing. An explicitly set `exp` field
    /// is never overwritten.
    ///
    /// The system auth flow passes [`Claims`](super::claims::Claims) here.
    /// Custom auth flows can pass any `Serialize` struct directly.
    ///
    /// # Errors
    ///
    /// Returns `Error::internal` with [`JwtError::SerializationFailed`](super::JwtError::SerializationFailed)
    /// if the payload cannot be serialized to JSON, or
    /// [`JwtError::SigningFailed`](super::JwtError::SigningFailed) if the HMAC signing
    /// operation fails.
    pub fn encode<T: Serialize>(&self, claims: &T) -> Result<String> {
        // Auto-fill exp if missing and default_expiry is configured
        let claims_json = if let Some(default_exp) = self.inner.default_expiry {
            let mut value = serde_json::to_value(claims).map_err(|_| {
                Error::internal("failed to serialize token")
                    .chain(JwtError::SerializationFailed)
                    .with_code(JwtError::SerializationFailed.code())
            })?;
            // Only inject exp when the payload has no exp field already
            if value.get("exp").is_none() {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("system clock before UNIX epoch")
                    .as_secs();
                value["exp"] = serde_json::Value::Number((now + default_exp.as_secs()).into());
            }
            serde_json::to_vec(&value)
        } else {
            serde_json::to_vec(claims)
        }
        .map_err(|_| {
            Error::internal("unauthorized")
                .chain(JwtError::SerializationFailed)
                .with_code(JwtError::SerializationFailed.code())
        })?;

        let alg = self.inner.signer.algorithm_name();
        let header = format!(r#"{{"alg":"{alg}","typ":"JWT"}}"#);
        let header_b64 = base64url::encode(header.as_bytes());
        let payload_b64 = base64url::encode(&claims_json);

        let header_payload = format!("{header_b64}.{payload_b64}");
        let signature = self.inner.signer.sign(header_payload.as_bytes())?;
        let signature_b64 = base64url::encode(&signature);

        Ok(format!("{header_payload}.{signature_b64}"))
    }
}

impl Clone for JwtEncoder {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    use super::super::claims::Claims;

    fn test_config() -> JwtSessionsConfig {
        JwtSessionsConfig {
            signing_secret: "test-secret-key-at-least-32-bytes-long!".into(),
            ..JwtSessionsConfig::default()
        }
    }

    #[test]
    fn encode_produces_three_part_token() {
        let encoder = JwtEncoder::from_config(&test_config());
        let claims = Claims::new().with_exp(9999999999);
        let token = encoder.encode(&claims).unwrap();
        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn encode_header_contains_hs256() {
        let encoder = JwtEncoder::from_config(&test_config());
        let claims = Claims::new().with_exp(9999999999);
        let token = encoder.encode(&claims).unwrap();
        let header_b64 = token.split('.').next().unwrap();
        let header_bytes = base64url::decode(header_b64).unwrap();
        let header: serde_json::Value = serde_json::from_slice(&header_bytes).unwrap();
        assert_eq!(header["alg"], "HS256");
        assert_eq!(header["typ"], "JWT");
    }

    #[test]
    fn encode_with_default_expiry_auto_sets_exp() {
        // access_ttl_secs is always used as default expiry — no manual override needed.
        let config = test_config(); // access_ttl_secs defaults to 900
        let encoder = JwtEncoder::from_config(&config);
        let claims = Claims::new(); // no exp — should be auto-filled from access_ttl_secs
        let token = encoder.encode(&claims).unwrap();
        let payload_b64 = token.split('.').nth(1).unwrap();
        let payload_bytes = base64url::decode(payload_b64).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&payload_bytes).unwrap();
        assert!(payload.get("exp").is_some());
    }

    #[test]
    fn encode_explicit_exp_not_overwritten() {
        let config = test_config();
        let encoder = JwtEncoder::from_config(&config);
        let claims = Claims::new().with_exp(42);
        let token = encoder.encode(&claims).unwrap();
        let payload_b64 = token.split('.').nth(1).unwrap();
        let payload_bytes = base64url::decode(payload_b64).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&payload_bytes).unwrap();
        assert_eq!(payload["exp"], 42);
    }

    #[test]
    fn encode_custom_struct_directly() {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct CustomPayload {
            sub: String,
            role: String,
            exp: u64,
        }

        let encoder = JwtEncoder::from_config(&test_config());
        let payload = CustomPayload {
            sub: "user_1".into(),
            role: "admin".into(),
            exp: 9999999999,
        };
        let token = encoder.encode(&payload).unwrap();
        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn clone_produces_working_encoder() {
        let encoder = JwtEncoder::from_config(&test_config());
        let cloned = encoder.clone();
        let claims = Claims::new().with_exp(9999999999);
        assert!(cloned.encode(&claims).is_ok());
    }
}
