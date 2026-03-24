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
