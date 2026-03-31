use std::fmt;

/// Typed JWT error enum. Stored as `modo::Error` source via `chain()`.
///
/// Use `error.source_as::<JwtError>()` before the response pipeline
/// or `error.error_code()` after `IntoResponse` to identify the failure.
///
/// # Error identity pattern
///
/// ```rust,ignore
/// use modo::auth::jwt::JwtError;
///
/// let err = modo::Error::unauthorized("unauthorized")
///     .chain(JwtError::Expired)
///     .with_code(JwtError::Expired.code());
///
/// // Before IntoResponse:
/// assert_eq!(err.source_as::<JwtError>(), Some(&JwtError::Expired));
/// // After IntoResponse (in error handler):
/// assert_eq!(err.error_code(), Some("jwt:expired"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JwtError {
    // Request errors (401)
    /// No token was found by any configured `TokenSource`.
    MissingToken,
    /// The token header could not be decoded or parsed.
    InvalidHeader,
    /// The token does not have the expected three-part structure.
    MalformedToken,
    /// The token payload could not be deserialized into the target claims type.
    DeserializationFailed,
    /// The token signature does not match the signing key.
    InvalidSignature,
    /// The token has expired (`exp` is in the past, beyond leeway).
    Expired,
    /// The token is not yet valid (`nbf` is in the future, beyond leeway).
    NotYetValid,
    /// The `iss` claim does not match the required issuer.
    InvalidIssuer,
    /// The `aud` claim does not match the required audience.
    InvalidAudience,
    /// The token's `jti` was found in the revocation store.
    Revoked,
    /// The revocation store returned an error (fail-closed).
    RevocationCheckFailed,
    /// The token header specifies an algorithm that differs from the verifier's algorithm.
    AlgorithmMismatch,
    // Server errors (500)
    /// The HMAC signing operation failed.
    SigningFailed,
    /// The claims could not be serialized to JSON.
    SerializationFailed,
}

impl JwtError {
    /// Returns a static error code string for use with `Error::with_code()`.
    ///
    /// Survives the `IntoResponse` → `Clone` → error handler pipeline where
    /// the original `source` is dropped.
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingToken => "jwt:missing_token",
            Self::InvalidHeader => "jwt:invalid_header",
            Self::MalformedToken => "jwt:malformed_token",
            Self::DeserializationFailed => "jwt:deserialization_failed",
            Self::InvalidSignature => "jwt:invalid_signature",
            Self::Expired => "jwt:expired",
            Self::NotYetValid => "jwt:not_yet_valid",
            Self::InvalidIssuer => "jwt:invalid_issuer",
            Self::InvalidAudience => "jwt:invalid_audience",
            Self::Revoked => "jwt:revoked",
            Self::RevocationCheckFailed => "jwt:revocation_check_failed",
            Self::AlgorithmMismatch => "jwt:algorithm_mismatch",
            Self::SigningFailed => "jwt:signing_failed",
            Self::SerializationFailed => "jwt:serialization_failed",
        }
    }
}

impl fmt::Display for JwtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingToken => write!(f, "missing token"),
            Self::InvalidHeader => write!(f, "invalid token header"),
            Self::MalformedToken => write!(f, "malformed token"),
            Self::DeserializationFailed => write!(f, "failed to deserialize token claims"),
            Self::InvalidSignature => write!(f, "invalid token signature"),
            Self::Expired => write!(f, "token has expired"),
            Self::NotYetValid => write!(f, "token is not yet valid"),
            Self::InvalidIssuer => write!(f, "invalid token issuer"),
            Self::InvalidAudience => write!(f, "invalid token audience"),
            Self::Revoked => write!(f, "token has been revoked"),
            Self::RevocationCheckFailed => write!(f, "token revocation check failed"),
            Self::AlgorithmMismatch => write!(f, "token algorithm mismatch"),
            Self::SigningFailed => write!(f, "failed to sign token"),
            Self::SerializationFailed => write!(f, "failed to serialize token claims"),
        }
    }
}

impl std::error::Error for JwtError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn all_variants_have_unique_codes() {
        let variants = [
            JwtError::MissingToken,
            JwtError::InvalidHeader,
            JwtError::MalformedToken,
            JwtError::DeserializationFailed,
            JwtError::InvalidSignature,
            JwtError::Expired,
            JwtError::NotYetValid,
            JwtError::InvalidIssuer,
            JwtError::InvalidAudience,
            JwtError::Revoked,
            JwtError::RevocationCheckFailed,
            JwtError::AlgorithmMismatch,
            JwtError::SigningFailed,
            JwtError::SerializationFailed,
        ];
        let mut codes: Vec<&str> = variants.iter().map(|v| v.code()).collect();
        let len_before = codes.len();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), len_before, "duplicate error codes found");
    }

    #[test]
    fn all_codes_start_with_jwt_prefix() {
        let variants = [
            JwtError::MissingToken,
            JwtError::Expired,
            JwtError::SigningFailed,
        ];
        for v in &variants {
            assert!(
                v.code().starts_with("jwt:"),
                "code {} missing prefix",
                v.code()
            );
        }
    }

    #[test]
    fn display_is_human_readable() {
        assert_eq!(JwtError::Expired.to_string(), "token has expired");
        assert_eq!(JwtError::MissingToken.to_string(), "missing token");
    }

    #[test]
    fn recoverable_via_source_as() {
        let err = Error::unauthorized("unauthorized")
            .chain(JwtError::Expired)
            .with_code(JwtError::Expired.code());
        let jwt_err = err.source_as::<JwtError>();
        assert_eq!(jwt_err, Some(&JwtError::Expired));
        assert_eq!(err.error_code(), Some("jwt:expired"));
    }
}
