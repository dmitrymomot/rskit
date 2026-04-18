use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::encoding::base64url;
use crate::{Error, Result};

use super::config::JwtSessionsConfig;
use super::encoder::JwtEncoder;
use super::error::JwtError;
use super::signer::{HmacSigner, TokenVerifier};
use super::validation::ValidationConfig;

/// JWT token decoder. Verifies signatures and validates claims.
///
/// All validation is synchronous. Cloning is cheap — state is stored behind `Arc`.
pub struct JwtDecoder {
    inner: Arc<JwtDecoderInner>,
}

struct JwtDecoderInner {
    verifier: Arc<dyn TokenVerifier>,
    validation: ValidationConfig,
}

pub(super) fn jwt_err(kind: JwtError) -> Error {
    let status_fn = match kind {
        JwtError::SigningFailed | JwtError::SerializationFailed => Error::internal,
        _ => Error::unauthorized,
    };
    status_fn("unauthorized").chain(kind).with_code(kind.code())
}

/// Build an `Error::unauthorized` carrying a stable error code string.
/// Used by middleware/extractors for `auth:*` codes that don't have a typed source.
pub(super) fn auth_err(code: &'static str) -> Error {
    Error::unauthorized("unauthorized").with_code(code)
}

impl JwtDecoder {
    /// Creates a `JwtDecoder` with an explicit verifier and validation policy.
    ///
    /// Use this constructor when you need full control over the validation
    /// config, e.g. to set `require_audience` or `leeway`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use std::sync::Arc;
    /// use modo::auth::session::jwt::{HmacSigner, JwtDecoder, ValidationConfig};
    ///
    /// let signer = HmacSigner::new(b"my-secret");
    /// let validation = ValidationConfig {
    ///     require_audience: Some("my-app".into()),
    ///     ..ValidationConfig::default()
    /// };
    /// let decoder = JwtDecoder::new(Arc::new(signer), validation);
    /// ```
    pub fn new(verifier: Arc<dyn TokenVerifier>, validation: ValidationConfig) -> Self {
        Self {
            inner: Arc::new(JwtDecoderInner {
                verifier,
                validation,
            }),
        }
    }

    /// Creates a `JwtDecoder` from YAML configuration.
    ///
    /// Uses `HmacSigner` (HS256) with the configured secret.
    pub fn from_config(config: &JwtSessionsConfig) -> Self {
        let signer = HmacSigner::new(config.signing_secret.as_bytes());
        Self {
            inner: Arc::new(JwtDecoderInner {
                verifier: Arc::new(signer),
                validation: ValidationConfig {
                    leeway: Duration::ZERO,
                    require_issuer: config.issuer.clone(),
                    require_audience: None,
                },
            }),
        }
    }

    /// Decodes and validates a JWT token string, returning `T`.
    ///
    /// The system auth flow passes [`Claims`](super::claims::Claims) as `T` and
    /// gets a `Claims` back. Custom auth flows can pass any
    /// `DeserializeOwned` struct directly.
    ///
    /// Validation order:
    /// 1. Split into 3 parts (`header.payload.signature`)
    /// 2. Decode header, check algorithm matches the verifier
    /// 3. Verify HMAC signature
    /// 4. Decode payload into JSON value
    /// 5. Enforce `exp` (always required; missing `exp` is treated as expired)
    /// 6. Check `nbf` (if present)
    /// 7. Check `iss` (if `require_issuer` is configured)
    /// 8. Check `aud` (if `require_audience` is configured)
    /// 9. Deserialize validated JSON value into `T`
    ///
    /// Clock skew tolerance (`leeway`) is applied to steps 5 and 6.
    ///
    /// # Errors
    ///
    /// Returns `Error::unauthorized` with a [`JwtError`](super::JwtError) source for:
    /// malformed tokens, invalid headers, algorithm mismatch, invalid signatures,
    /// expired tokens, not-yet-valid tokens, issuer mismatch, or audience mismatch.
    /// Missing `exp` is treated as expired.
    pub fn decode<T: DeserializeOwned>(&self, token: &str) -> Result<T> {
        let mut iter = token.splitn(4, '.');
        let (Some(header_b64), Some(payload_b64), Some(signature_b64), None) =
            (iter.next(), iter.next(), iter.next(), iter.next())
        else {
            return Err(jwt_err(JwtError::MalformedToken));
        };

        let header_bytes =
            base64url::decode(header_b64).map_err(|_| jwt_err(JwtError::InvalidHeader))?;
        let header: serde_json::Value =
            serde_json::from_slice(&header_bytes).map_err(|_| jwt_err(JwtError::InvalidHeader))?;

        let alg = header["alg"]
            .as_str()
            .ok_or_else(|| jwt_err(JwtError::InvalidHeader))?;
        if alg != self.inner.verifier.algorithm_name() {
            return Err(jwt_err(JwtError::AlgorithmMismatch));
        }

        let signature =
            base64url::decode(signature_b64).map_err(|_| jwt_err(JwtError::MalformedToken))?;
        let mut header_payload = Vec::with_capacity(header_b64.len() + 1 + payload_b64.len());
        header_payload.extend_from_slice(header_b64.as_bytes());
        header_payload.push(b'.');
        header_payload.extend_from_slice(payload_b64.as_bytes());
        self.inner.verifier.verify(&header_payload, &signature)?;

        let payload_bytes =
            base64url::decode(payload_b64).map_err(|_| jwt_err(JwtError::MalformedToken))?;
        let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)
            .map_err(|_| jwt_err(JwtError::DeserializationFailed))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before UNIX epoch")
            .as_secs();
        let leeway = self.inner.validation.leeway.as_secs();

        let exp = payload
            .get("exp")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| jwt_err(JwtError::Expired))?;
        if now > exp + leeway {
            return Err(jwt_err(JwtError::Expired));
        }

        if let Some(nbf) = payload.get("nbf").and_then(|v| v.as_u64())
            && now + leeway < nbf
        {
            return Err(jwt_err(JwtError::NotYetValid));
        }

        if let Some(ref required_iss) = self.inner.validation.require_issuer {
            match payload.get("iss").and_then(|v| v.as_str()) {
                Some(iss) if iss == required_iss => {}
                _ => return Err(jwt_err(JwtError::InvalidIssuer)),
            }
        }

        if let Some(ref required_aud) = self.inner.validation.require_audience {
            match payload.get("aud").and_then(|v| v.as_str()) {
                Some(aud) if aud == required_aud => {}
                _ => return Err(jwt_err(JwtError::InvalidAudience)),
            }
        }

        serde_json::from_value(payload).map_err(|_| jwt_err(JwtError::DeserializationFailed))
    }
}

/// Creates a `JwtDecoder` that shares the signing key and validation config
/// of an existing `JwtEncoder`. Useful when encoder and decoder are wired
/// from the same `JwtConfig` value.
impl From<&JwtEncoder> for JwtDecoder {
    fn from(encoder: &JwtEncoder) -> Self {
        Self {
            inner: Arc::new(JwtDecoderInner {
                verifier: encoder.verifier(),
                validation: encoder.validation(),
            }),
        }
    }
}

impl Clone for JwtDecoder {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    use super::super::claims::Claims;
    use super::super::encoder::JwtEncoder;

    fn test_config() -> JwtSessionsConfig {
        JwtSessionsConfig {
            signing_secret: "test-secret-key-at-least-32-bytes-long!".into(),
            ..JwtSessionsConfig::default()
        }
    }

    fn encode_decode_config() -> (JwtEncoder, JwtDecoder) {
        let config = test_config();
        let encoder = JwtEncoder::from_config(&config);
        let decoder = JwtDecoder::from_config(&config);
        (encoder, decoder)
    }

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[test]
    fn encode_decode_roundtrip() {
        let (encoder, decoder) = encode_decode_config();
        let claims = Claims::new().with_sub("user_1").with_exp(now_secs() + 3600);
        let token = encoder.encode(&claims).unwrap();
        let decoded: Claims = decoder.decode(&token).unwrap();
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.exp, claims.exp);
    }

    #[test]
    fn rejects_expired_token() {
        let (encoder, decoder) = encode_decode_config();
        let claims = Claims::new().with_exp(now_secs() - 10);
        let token = encoder.encode(&claims).unwrap();
        let err = decoder.decode::<Claims>(&token).unwrap_err();
        assert_eq!(err.error_code(), Some("jwt:expired"));
    }

    #[test]
    fn respects_leeway_for_exp() {
        // leeway is 0 in the new config — a token expired even 1s ago is rejected.
        let (encoder, decoder) = encode_decode_config();
        let claims = Claims::new().with_exp(now_secs() - 10);
        let token = encoder.encode(&claims).unwrap();
        // With leeway=0, a token expired 10s ago must be rejected.
        let err = decoder.decode::<Claims>(&token).unwrap_err();
        assert_eq!(err.error_code(), Some("jwt:expired"));
    }

    #[test]
    fn rejects_token_before_nbf() {
        let (encoder, decoder) = encode_decode_config();
        let claims = Claims::new()
            .with_exp(now_secs() + 3600)
            .with_nbf(now_secs() + 3600);
        let token = encoder.encode(&claims).unwrap();
        let err = decoder.decode::<Claims>(&token).unwrap_err();
        assert_eq!(err.error_code(), Some("jwt:not_yet_valid"));
    }

    #[test]
    fn rejects_wrong_issuer() {
        let mut config = test_config();
        config.issuer = Some("expected-app".into());
        let encoder = JwtEncoder::from_config(&config);
        let decoder = JwtDecoder::from_config(&config);
        let claims = Claims::new()
            .with_exp(now_secs() + 3600)
            .with_iss("wrong-app");
        let token = encoder.encode(&claims).unwrap();
        let err = decoder.decode::<Claims>(&token).unwrap_err();
        assert_eq!(err.error_code(), Some("jwt:invalid_issuer"));
    }

    #[test]
    fn rejects_missing_issuer_when_required() {
        let mut config = test_config();
        config.issuer = Some("expected-app".into());
        let encoder = JwtEncoder::from_config(&config);
        let decoder = JwtDecoder::from_config(&config);
        let claims = Claims::new().with_exp(now_secs() + 3600);
        let token = encoder.encode(&claims).unwrap();
        let err = decoder.decode::<Claims>(&token).unwrap_err();
        assert_eq!(err.error_code(), Some("jwt:invalid_issuer"));
    }

    #[test]
    fn accepts_when_no_issuer_policy() {
        let (encoder, decoder) = encode_decode_config();
        let claims = Claims::new()
            .with_exp(now_secs() + 3600)
            .with_iss("any-app");
        let token = encoder.encode(&claims).unwrap();
        assert!(decoder.decode::<Claims>(&token).is_ok());
    }

    #[test]
    fn rejects_wrong_audience() {
        let config = test_config();
        let encoder = JwtEncoder::from_config(&config);
        let signer = HmacSigner::new(config.signing_secret.as_bytes());
        let validation = super::super::validation::ValidationConfig::default()
            .with_audience("expected-audience");
        let decoder = JwtDecoder::new(Arc::new(signer), validation);
        let claims = Claims::new()
            .with_exp(now_secs() + 3600)
            .with_aud("wrong-audience");
        let token = encoder.encode(&claims).unwrap();
        let err = decoder.decode::<Claims>(&token).unwrap_err();
        assert_eq!(err.error_code(), Some("jwt:invalid_audience"));
    }

    #[test]
    fn rejects_tampered_signature() {
        let (encoder, decoder) = encode_decode_config();
        let claims = Claims::new().with_exp(now_secs() + 3600);
        let mut token = encoder.encode(&claims).unwrap();
        // Flip a character well inside the signature (not in base64 padding region)
        let idx = token.len() - 5;
        let original = token.as_bytes()[idx];
        let replacement = if original == b'A' { b'B' } else { b'A' };
        // SAFETY: replacing one ASCII byte with another ASCII byte
        unsafe { token.as_bytes_mut()[idx] = replacement };
        let err = decoder.decode::<Claims>(&token).unwrap_err();
        assert_eq!(err.error_code(), Some("jwt:invalid_signature"));
    }

    #[test]
    fn rejects_malformed_token() {
        let decoder = JwtDecoder::from_config(&test_config());
        let err = decoder
            .decode::<Claims>("not.a.valid.token.at.all")
            .unwrap_err();
        assert_eq!(err.error_code(), Some("jwt:malformed_token"));
    }

    #[test]
    fn rejects_token_with_wrong_algorithm() {
        let (encoder, _) = encode_decode_config();
        let claims = Claims::new().with_exp(now_secs() + 3600);
        let token = encoder.encode(&claims).unwrap();
        // Replace HS256 with RS256 in the header
        let parts: Vec<&str> = token.splitn(3, '.').collect();
        let header_bytes = base64url::decode(parts[0]).unwrap();
        let header_str = String::from_utf8(header_bytes).unwrap();
        let tampered_header = header_str.replace("HS256", "RS256");
        let tampered_header_b64 = base64url::encode(tampered_header.as_bytes());
        let tampered_token = format!("{}.{}.{}", tampered_header_b64, parts[1], parts[2]);
        let decoder = JwtDecoder::from_config(&test_config());
        let err = decoder.decode::<Claims>(&tampered_token).unwrap_err();
        assert_eq!(err.error_code(), Some("jwt:algorithm_mismatch"));
    }

    #[test]
    fn rejects_missing_exp() {
        // JwtEncoder::from_config() always injects exp via access_ttl_secs,
        // so a token without exp can only be produced via the signer directly.
        // The decoder still rejects tokens without exp — verified here via a
        // manually crafted token.
        use super::super::signer::{HmacSigner, TokenSigner};
        use crate::encoding::base64url;

        let config = test_config();
        let signer = HmacSigner::new(config.signing_secret.as_bytes());
        let header = base64url::encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = base64url::encode(br#"{"sub":"user_1"}"#); // no exp
        let header_payload = format!("{header}.{payload}");
        let sig = signer.sign(header_payload.as_bytes()).unwrap();
        let sig_b64 = base64url::encode(&sig);
        let token = format!("{header_payload}.{sig_b64}");

        let decoder = JwtDecoder::from_config(&config);
        let err = decoder.decode::<Claims>(&token).unwrap_err();
        assert_eq!(err.error_code(), Some("jwt:expired"));
    }

    #[test]
    fn from_encoder_shares_verifier() {
        let config = test_config();
        let encoder = JwtEncoder::from_config(&config);
        let decoder = JwtDecoder::from(&encoder);
        let claims = Claims::new().with_exp(now_secs() + 3600);
        let token = encoder.encode(&claims).unwrap();
        assert!(decoder.decode::<Claims>(&token).is_ok());
    }

    #[test]
    fn decode_custom_struct_directly() {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        struct CustomPayload {
            sub: String,
            role: String,
            exp: u64,
        }

        let (encoder, decoder) = encode_decode_config();
        let payload = CustomPayload {
            sub: "user_1".into(),
            role: "admin".into(),
            exp: now_secs() + 3600,
        };
        let token = encoder.encode(&payload).unwrap();
        let decoded: CustomPayload = decoder.decode(&token).unwrap();
        assert_eq!(decoded.sub, "user_1");
        assert_eq!(decoded.role, "admin");
    }
}
