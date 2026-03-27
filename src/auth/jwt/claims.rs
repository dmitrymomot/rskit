use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// JWT claims with all seven registered claims plus user-defined custom fields.
///
/// All registered claims are optional — `None` values are omitted from the
/// serialized token. Custom fields are flattened into the top-level JSON object.
///
/// # Example
///
/// ```
/// use modo::auth::jwt::Claims;
/// use serde::{Serialize, Deserialize};
/// use std::time::Duration;
///
/// #[derive(Serialize, Deserialize, Clone)]
/// struct MyClaims {
///     role: String,
/// }
///
/// let claims = Claims::new(MyClaims { role: "admin".into() })
///     .with_sub("user_123")
///     .with_iat_now()
///     .with_exp_in(Duration::from_secs(3600));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims<T> {
    /// Issuer (`iss`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    /// Subject (`sub`) — typically the user identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    /// Audience (`aud`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    /// Expiration time (`exp`) as a Unix timestamp in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<u64>,
    /// Not-before time (`nbf`) as a Unix timestamp in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf: Option<u64>,
    /// Issued-at time (`iat`) as a Unix timestamp in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<u64>,
    /// JWT ID (`jti`) — unique identifier for the token, used for revocation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    /// Application-defined custom claims, flattened into the top-level JSON object.
    #[serde(flatten)]
    pub custom: T,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs()
}

impl<T> Claims<T> {
    /// Creates a new `Claims` with all registered fields set to `None`.
    pub fn new(custom: T) -> Self {
        Self {
            iss: None,
            sub: None,
            aud: None,
            exp: None,
            nbf: None,
            iat: None,
            jti: None,
            custom,
        }
    }

    /// Sets the issuer (`iss`) claim.
    pub fn with_iss(mut self, iss: impl Into<String>) -> Self {
        self.iss = Some(iss.into());
        self
    }

    /// Sets the subject (`sub`) claim.
    pub fn with_sub(mut self, sub: impl Into<String>) -> Self {
        self.sub = Some(sub.into());
        self
    }

    /// Sets the audience (`aud`) claim.
    pub fn with_aud(mut self, aud: impl Into<String>) -> Self {
        self.aud = Some(aud.into());
        self
    }

    /// Sets the expiration time (`exp`) as an absolute Unix timestamp in seconds.
    pub fn with_exp(mut self, exp: u64) -> Self {
        self.exp = Some(exp);
        self
    }

    /// Sets the expiration time (`exp`) relative to the current time.
    pub fn with_exp_in(mut self, duration: Duration) -> Self {
        self.exp = Some(now_secs() + duration.as_secs());
        self
    }

    /// Sets the not-before time (`nbf`) as an absolute Unix timestamp in seconds.
    pub fn with_nbf(mut self, nbf: u64) -> Self {
        self.nbf = Some(nbf);
        self
    }

    /// Sets the issued-at time (`iat`) to the current time.
    pub fn with_iat_now(mut self) -> Self {
        self.iat = Some(now_secs());
        self
    }

    /// Sets the JWT ID (`jti`). Required for revocation checks.
    pub fn with_jti(mut self, jti: impl Into<String>) -> Self {
        self.jti = Some(jti.into());
        self
    }

    /// Returns `true` if the token has an `exp` claim that is in the past.
    /// Returns `false` when `exp` is absent.
    pub fn is_expired(&self) -> bool {
        match self.exp {
            Some(exp) => now_secs() > exp,
            None => false,
        }
    }

    /// Returns `true` if the token has an `nbf` claim that is in the future.
    /// Returns `false` when `nbf` is absent.
    pub fn is_not_yet_valid(&self) -> bool {
        match self.nbf {
            Some(nbf) => now_secs() < nbf,
            None => false,
        }
    }

    /// Returns the subject claim (`sub`) as a string slice, if present.
    pub fn subject(&self) -> Option<&str> {
        self.sub.as_deref()
    }

    /// Returns the JWT ID (`jti`) as a string slice, if present.
    pub fn token_id(&self) -> Option<&str> {
        self.jti.as_deref()
    }

    /// Returns the issuer claim (`iss`) as a string slice, if present.
    pub fn issuer(&self) -> Option<&str> {
        self.iss.as_deref()
    }

    /// Returns the audience claim (`aud`) as a string slice, if present.
    pub fn audience(&self) -> Option<&str> {
        self.aud.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct Custom {
        role: String,
    }

    #[test]
    fn new_sets_custom_and_none_claims() {
        let claims = Claims::new(Custom {
            role: "admin".into(),
        });
        assert_eq!(claims.custom.role, "admin");
        assert!(claims.iss.is_none());
        assert!(claims.sub.is_none());
        assert!(claims.exp.is_none());
    }

    #[test]
    fn builders_set_fields() {
        let claims = Claims::new(())
            .with_sub("user_1")
            .with_iss("my-app")
            .with_aud("api")
            .with_exp(9999999999)
            .with_nbf(1000000000)
            .with_jti("token-id");
        assert_eq!(claims.subject(), Some("user_1"));
        assert_eq!(claims.issuer(), Some("my-app"));
        assert_eq!(claims.audience(), Some("api"));
        assert_eq!(claims.exp, Some(9999999999));
        assert_eq!(claims.nbf, Some(1000000000));
        assert_eq!(claims.token_id(), Some("token-id"));
    }

    #[test]
    fn with_exp_in_sets_future_timestamp() {
        let claims = Claims::new(()).with_exp_in(Duration::from_secs(3600));
        let exp = claims.exp.unwrap();
        let now = now_secs();
        assert!(exp >= now + 3599 && exp <= now + 3601);
    }

    #[test]
    fn with_iat_now_sets_current_timestamp() {
        let claims = Claims::new(()).with_iat_now();
        let iat = claims.iat.unwrap();
        let now = now_secs();
        assert!(iat >= now - 1 && iat <= now + 1);
    }

    #[test]
    fn is_expired_returns_false_for_future_exp() {
        let claims = Claims::new(()).with_exp(now_secs() + 3600);
        assert!(!claims.is_expired());
    }

    #[test]
    fn is_expired_returns_true_for_past_exp() {
        let claims = Claims::new(()).with_exp(now_secs() - 1);
        assert!(claims.is_expired());
    }

    #[test]
    fn is_expired_returns_false_when_no_exp() {
        let claims = Claims::<()>::new(());
        assert!(!claims.is_expired());
    }

    #[test]
    fn is_not_yet_valid_returns_true_for_future_nbf() {
        let claims = Claims::new(()).with_nbf(now_secs() + 3600);
        assert!(claims.is_not_yet_valid());
    }

    #[test]
    fn is_not_yet_valid_returns_false_for_past_nbf() {
        let claims = Claims::new(()).with_nbf(now_secs() - 1);
        assert!(!claims.is_not_yet_valid());
    }

    #[test]
    fn serialization_skips_none_fields() {
        let claims = Claims::new(Custom {
            role: "admin".into(),
        })
        .with_sub("user_1");
        let json = serde_json::to_value(&claims).unwrap();
        assert!(json.get("sub").is_some());
        assert!(json.get("iss").is_none());
        assert!(json.get("exp").is_none());
        assert_eq!(json["role"], "admin"); // flattened
    }

    #[test]
    fn flatten_merges_custom_at_top_level() {
        let claims = Claims::new(Custom {
            role: "editor".into(),
        });
        let json = serde_json::to_value(&claims).unwrap();
        assert_eq!(json["role"], "editor");
        assert!(json.get("custom").is_none()); // not nested
    }

    #[test]
    fn deserialization_roundtrip() {
        let original = Claims::new(Custom {
            role: "admin".into(),
        })
        .with_sub("user_1")
        .with_exp(9999999999);
        let json = serde_json::to_string(&original).unwrap();
        let decoded: Claims<Custom> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.sub, original.sub);
        assert_eq!(decoded.exp, original.exp);
        assert_eq!(decoded.custom, original.custom);
    }
}
