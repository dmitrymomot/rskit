use std::time::Duration;

/// Policy-level validation rules applied to every `decode()` call.
///
/// `exp` is always enforced (not configurable). These rules control
/// additional checks for `iss`, `aud`, and clock skew tolerance.
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub leeway: Duration,
    pub require_issuer: Option<String>,
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
