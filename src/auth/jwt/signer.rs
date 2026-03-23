use std::sync::Arc;

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::{Error, Result};

use super::error::JwtError;

type HmacSha256 = Hmac<Sha256>;

/// Object-safe trait for JWT signature verification.
pub trait TokenVerifier: Send + Sync {
    fn verify(&self, header_payload: &[u8], signature: &[u8]) -> Result<()>;
    fn algorithm_name(&self) -> &str;
}

/// Extends `TokenVerifier` with signing capability.
pub trait TokenSigner: TokenVerifier {
    fn sign(&self, header_payload: &[u8]) -> Result<Vec<u8>>;
}

/// HS256 (HMAC-SHA256) implementation of `TokenSigner`.
pub struct HmacSigner {
    inner: Arc<HmacSignerInner>,
}

struct HmacSignerInner {
    secret: Vec<u8>,
}

impl HmacSigner {
    pub fn new(secret: impl AsRef<[u8]>) -> Self {
        Self {
            inner: Arc::new(HmacSignerInner {
                secret: secret.as_ref().to_vec(),
            }),
        }
    }
}

impl Clone for HmacSigner {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl TokenVerifier for HmacSigner {
    fn verify(&self, header_payload: &[u8], signature: &[u8]) -> Result<()> {
        let mut mac = HmacSha256::new_from_slice(&self.inner.secret)
            .map_err(|_| Error::internal("invalid HMAC key").chain(JwtError::InvalidSignature))?;
        mac.update(header_payload);
        mac.verify_slice(signature).map_err(|_| {
            Error::unauthorized("unauthorized")
                .chain(JwtError::InvalidSignature)
                .with_code(JwtError::InvalidSignature.code())
        })
    }

    fn algorithm_name(&self) -> &str {
        "HS256"
    }
}

impl TokenSigner for HmacSigner {
    fn sign(&self, header_payload: &[u8]) -> Result<Vec<u8>> {
        let mut mac = HmacSha256::new_from_slice(&self.inner.secret)
            .map_err(|_| Error::internal("invalid HMAC key").chain(JwtError::SigningFailed))?;
        mac.update(header_payload);
        Ok(mac.finalize().into_bytes().to_vec())
    }
}

impl From<HmacSigner> for Arc<dyn TokenSigner> {
    fn from(signer: HmacSigner) -> Self {
        Arc::new(signer)
    }
}

impl From<HmacSigner> for Arc<dyn TokenVerifier> {
    fn from(signer: HmacSigner) -> Self {
        Arc::new(signer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_roundtrip() {
        let signer = HmacSigner::new(b"secret-key");
        let data = b"header.payload";
        let sig = signer.sign(data).unwrap();
        assert!(signer.verify(data, &sig).is_ok());
    }

    #[test]
    fn verify_rejects_tampered_payload() {
        let signer = HmacSigner::new(b"secret-key");
        let sig = signer.sign(b"header.payload").unwrap();
        let result = signer.verify(b"header.tampered", &sig);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status(), http::StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let signer1 = HmacSigner::new(b"secret-one");
        let signer2 = HmacSigner::new(b"secret-two");
        let sig = signer1.sign(b"data").unwrap();
        assert!(signer2.verify(b"data", &sig).is_err());
    }

    #[test]
    fn algorithm_name_returns_hs256() {
        let signer = HmacSigner::new(b"key");
        assert_eq!(signer.algorithm_name(), "HS256");
    }

    #[test]
    fn clone_shares_inner() {
        let signer = HmacSigner::new(b"key");
        let cloned = signer.clone();
        let sig = signer.sign(b"data").unwrap();
        assert!(cloned.verify(b"data", &sig).is_ok());
    }

    #[test]
    fn into_arc_dyn_token_signer() {
        let signer = HmacSigner::new(b"key");
        let arc_signer: Arc<dyn TokenSigner> = signer.into();
        assert_eq!(arc_signer.algorithm_name(), "HS256");
    }

    #[test]
    fn into_arc_dyn_token_verifier() {
        let signer = HmacSigner::new(b"key");
        let arc_verifier: Arc<dyn TokenVerifier> = signer.into();
        assert_eq!(arc_verifier.algorithm_name(), "HS256");
    }
}
