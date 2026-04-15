use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;

use crate::encoding::base64url;
use crate::{Error, Result};

use super::config::JwtConfig;
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
    /// config (leeway, issuer, audience) is stored so a matching `JwtDecoder`
    /// can be created via `JwtDecoder::from(&encoder)`.
    pub fn from_config(config: &JwtConfig) -> Self {
        let signer = HmacSigner::new(config.secret.as_bytes());
        Self {
            inner: Arc::new(JwtEncoderInner {
                signer: Arc::new(signer),
                default_expiry: config.default_expiry.map(Duration::from_secs),
                validation: super::validation::ValidationConfig {
                    leeway: Duration::from_secs(config.leeway),
                    require_issuer: config.issuer.clone(),
                    require_audience: config.audience.clone(),
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

    /// Encodes claims into a signed JWT token string.
    ///
    /// If `claims.exp` is `None` and `default_expiry` is configured,
    /// `exp` is automatically set to `now + default_expiry` before signing.
    /// An explicitly set `exp` is never overwritten.
    ///
    /// # Errors
    ///
    /// Returns `Error::internal` with [`JwtError::SerializationFailed`](super::JwtError::SerializationFailed)
    /// if the claims cannot be serialized to JSON, or
    /// [`JwtError::SigningFailed`](super::JwtError::SigningFailed) if the HMAC signing
    /// operation fails.
    pub fn encode<T: Serialize>(&self, claims: &super::claims::Claims<T>) -> Result<String> {
        // Auto-fill exp if missing and default_expiry is configured
        let claims_json = if claims.exp.is_none() {
            if let Some(default_exp) = self.inner.default_expiry {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("system clock before UNIX epoch")
                    .as_secs();
                let mut value = serde_json::to_value(claims).map_err(|_| {
                    Error::internal("failed to serialize token")
                        .chain(JwtError::SerializationFailed)
                        .with_code(JwtError::SerializationFailed.code())
                })?;
                value["exp"] = serde_json::Value::Number((now + default_exp.as_secs()).into());
                serde_json::to_vec(&value)
            } else {
                serde_json::to_vec(claims)
            }
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

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestClaims {
        role: String,
    }

    fn test_config() -> JwtConfig {
        JwtConfig {
            secret: "test-secret-key-at-least-32-bytes-long!".into(),
            default_expiry: None,
            leeway: 0,
            issuer: None,
            audience: None,
        }
    }

    #[test]
    fn encode_produces_three_part_token() {
        let encoder = JwtEncoder::from_config(&test_config());
        let claims = super::super::claims::Claims::new(TestClaims {
            role: "admin".into(),
        })
        .with_exp(9999999999);
        let token = encoder.encode(&claims).unwrap();
        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn encode_header_contains_hs256() {
        let encoder = JwtEncoder::from_config(&test_config());
        let claims = super::super::claims::Claims::new(()).with_exp(9999999999);
        let token = encoder.encode(&claims).unwrap();
        let header_b64 = token.split('.').next().unwrap();
        let header_bytes = base64url::decode(header_b64).unwrap();
        let header: serde_json::Value = serde_json::from_slice(&header_bytes).unwrap();
        assert_eq!(header["alg"], "HS256");
        assert_eq!(header["typ"], "JWT");
    }

    #[test]
    fn encode_with_default_expiry_auto_sets_exp() {
        let mut config = test_config();
        config.default_expiry = Some(3600);
        let encoder = JwtEncoder::from_config(&config);
        let claims = super::super::claims::Claims::new(());
        // claims.exp is None — should be auto-filled
        let token = encoder.encode(&claims).unwrap();
        let payload_b64 = token.split('.').nth(1).unwrap();
        let payload_bytes = base64url::decode(payload_b64).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&payload_bytes).unwrap();
        assert!(payload.get("exp").is_some());
    }

    #[test]
    fn encode_explicit_exp_not_overwritten() {
        let mut config = test_config();
        config.default_expiry = Some(3600);
        let encoder = JwtEncoder::from_config(&config);
        let claims = super::super::claims::Claims::new(()).with_exp(42);
        let token = encoder.encode(&claims).unwrap();
        let payload_b64 = token.split('.').nth(1).unwrap();
        let payload_bytes = base64url::decode(payload_b64).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&payload_bytes).unwrap();
        assert_eq!(payload["exp"], 42);
    }

    #[test]
    fn clone_produces_working_encoder() {
        let encoder = JwtEncoder::from_config(&test_config());
        let cloned = encoder.clone();
        let claims = super::super::claims::Claims::new(()).with_exp(9999999999);
        assert!(cloned.encode(&claims).is_ok());
    }
}
