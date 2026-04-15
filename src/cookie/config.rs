use serde::Deserialize;

fn default_true() -> bool {
    true
}

fn default_lax() -> String {
    "lax".to_string()
}

/// Cookie security attributes used by the session and flash middleware.
///
/// Deserializes from the `cookie` section of the application YAML config.
/// All fields except `secret` have defaults, so a minimal config only needs
/// to provide `secret`.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
pub struct CookieConfig {
    /// HMAC signing secret. Must be at least 64 characters long.
    pub secret: String,
    /// Set the `Secure` cookie attribute. Defaults to `true`.
    ///
    /// Set to `false` during local HTTP development.
    #[serde(default = "default_true")]
    pub secure: bool,
    /// Set the `HttpOnly` cookie attribute. Defaults to `true`.
    #[serde(default = "default_true")]
    pub http_only: bool,
    /// `SameSite` cookie attribute value: `"lax"`, `"strict"`, or `"none"`.
    /// Defaults to `"lax"`.
    #[serde(default = "default_lax")]
    pub same_site: String,
}

impl Default for CookieConfig {
    fn default() -> Self {
        Self {
            secret: String::new(),
            secure: true,
            http_only: true,
            same_site: "lax".to_string(),
        }
    }
}

impl CookieConfig {
    /// Create a new cookie configuration with the given signing secret.
    ///
    /// Defaults: `secure = true`, `http_only = true`, `same_site = "lax"`.
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
            secure: true,
            http_only: true,
            same_site: "lax".to_string(),
        }
    }
}
