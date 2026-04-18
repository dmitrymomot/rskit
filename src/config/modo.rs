use serde::Deserialize;

/// Top-level framework configuration.
///
/// Deserializes from a YAML file loaded by [`crate::config::load`]. All fields
/// use `#[serde(default)]`, so any section omitted from the YAML file falls
/// back to the type's own `Default` implementation.
///
/// Applications that need extra config fields can embed `Config` with
/// `#[serde(flatten)]` inside their own config struct.
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    /// HTTP server bind address and shutdown behaviour.
    pub server: crate::server::Config,
    /// libsql database settings.
    #[serde(default)]
    pub database: crate::db::Config,
    /// Log level, format, and optional Sentry integration.
    pub tracing: crate::tracing::Config,
    /// Signed cookie secret and attributes. When absent, signed/private cookies
    /// are disabled.
    pub cookie: Option<crate::cookie::CookieConfig>,
    /// HTTP security-header middleware settings.
    pub security_headers: crate::middleware::SecurityHeadersConfig,
    /// CORS policy.
    pub cors: crate::middleware::CorsConfig,
    /// CSRF protection settings.
    pub csrf: crate::middleware::CsrfConfig,
    /// Token-bucket rate-limiting settings.
    pub rate_limit: crate::middleware::RateLimitConfig,
    /// Session TTL, cookie name, fingerprint validation, touch interval, and
    /// per-user session limit.
    #[serde(default)]
    pub session: crate::auth::session::SessionConfig,
    /// Background job queue settings.
    #[serde(default)]
    pub job: crate::job::JobConfig,
    /// CIDR ranges of trusted reverse proxies used by [`crate::ip::ClientIpLayer`].
    ///
    /// Accepts any string parseable as [`ipnet::IpNet`], e.g. `"10.0.0.0/8"`.
    #[serde(default)]
    pub trusted_proxies: Vec<String>,
    /// OAuth provider settings.
    #[serde(default)]
    pub oauth: crate::auth::oauth::OAuthConfig,
    /// SMTP / email delivery settings.
    #[serde(default)]
    pub email: crate::email::EmailConfig,
    /// MiniJinja template engine settings.
    #[serde(default)]
    pub template: crate::template::TemplateConfig,
    /// Internationalization locale resolver chain and translation store settings.
    #[serde(default)]
    pub i18n: crate::i18n::I18nConfig,
    /// MaxMind GeoIP database path and settings.
    #[serde(default)]
    pub geolocation: crate::geolocation::GeolocationConfig,
    /// S3-compatible storage bucket settings.
    #[serde(default)]
    pub storage: crate::storage::BucketConfig,
    /// DNS verification settings.
    #[serde(default)]
    pub dns: crate::dns::DnsConfig,
    /// API key module settings.
    #[serde(default)]
    pub apikey: crate::auth::apikey::ApiKeyConfig,
    /// JWT session signing and validation settings.
    #[serde(default)]
    pub jwt: crate::auth::session::jwt::JwtSessionsConfig,
}
