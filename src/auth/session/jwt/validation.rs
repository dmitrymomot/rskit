use std::time::Duration;

/// Policy-level validation rules applied to every `decode()` call.
///
/// `exp` is always enforced (not configurable). These fields control
/// additional checks for `iss`, `aud`, and clock skew tolerance.
///
/// Built automatically from [`JwtSessionsConfig`](super::config::JwtSessionsConfig) by
/// `JwtEncoder::from_config()` and `JwtDecoder::from_config()`.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Allowed clock skew applied to `exp` and `nbf` checks.
    /// Defaults to `Duration::ZERO`.
    pub leeway: Duration,
    /// When `Some`, `decode()` rejects tokens whose `iss` does not match.
    pub require_issuer: Option<String>,
    /// When `Some`, `decode()` rejects tokens whose `aud` does not match.
    pub require_audience: Option<String>,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            leeway: Duration::ZERO,
            require_issuer: None,
            require_audience: None,
        }
    }
}

impl ValidationConfig {
    /// Require that decoded tokens carry a specific `aud` claim.
    ///
    /// Returns a new `ValidationConfig` with `require_audience` set. All other
    /// fields are unchanged.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let validation = ValidationConfig::default().with_audience("my-app");
    /// ```
    #[must_use]
    pub fn with_audience(mut self, aud: impl Into<String>) -> Self {
        self.require_audience = Some(aud.into());
        self
    }

    /// Require that decoded tokens carry a specific `iss` claim.
    ///
    /// Returns a new `ValidationConfig` with `require_issuer` set. All other
    /// fields are unchanged.
    #[must_use]
    pub fn with_issuer(mut self, iss: impl Into<String>) -> Self {
        self.require_issuer = Some(iss.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_zero_leeway_and_no_requirements() {
        let config = ValidationConfig::default();
        assert_eq!(config.leeway, Duration::ZERO);
        assert!(config.require_issuer.is_none());
        assert!(config.require_audience.is_none());
    }
}
