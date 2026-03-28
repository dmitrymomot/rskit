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
    /// SQLite connection pool settings. Requires the `db` feature.
    #[cfg(feature = "db")]
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
    /// per-user session limit. Requires the `db` feature.
    #[cfg(feature = "db")]
    #[serde(default)]
    pub session: crate::session::SessionConfig,
    /// Pagination defaults (items per page, max page size). Requires the `db` feature.
    #[cfg(feature = "db")]
    #[serde(default)]
    pub pagination: crate::page::PaginationConfig,
    /// HTTP client settings (timeout, retries, user agent).
    /// Requires the `http-client` feature.
    #[cfg(feature = "http-client")]
    #[serde(default)]
    pub http: crate::http::ClientConfig,
    /// Background job queue settings. Requires the `db` feature.
    #[cfg(feature = "db")]
    #[serde(default)]
    pub job: crate::job::JobConfig,
    /// CIDR ranges of trusted reverse proxies used by [`crate::ip::ClientIpLayer`].
    ///
    /// Accepts any string parseable as [`ipnet::IpNet`], e.g. `"10.0.0.0/8"`.
    #[serde(default)]
    pub trusted_proxies: Vec<String>,
    /// OAuth provider settings. Requires the `auth` feature.
    #[cfg(feature = "auth")]
    #[serde(default)]
    pub oauth: crate::auth::oauth::OAuthConfig,
    /// SMTP / email delivery settings. Requires the `email` feature.
    #[cfg(feature = "email")]
    #[serde(default)]
    pub email: crate::email::EmailConfig,
    /// MiniJinja template engine settings. Requires the `templates` feature.
    #[cfg(feature = "templates")]
    #[serde(default)]
    pub template: crate::template::TemplateConfig,
    /// MaxMind GeoIP database path and settings. Requires the `geolocation`
    /// feature.
    #[cfg(feature = "geolocation")]
    #[serde(default)]
    pub geolocation: crate::geolocation::GeolocationConfig,
    /// S3-compatible storage bucket settings. Requires the `storage` feature.
    #[cfg(feature = "storage")]
    #[serde(default)]
    pub storage: crate::storage::BucketConfig,
    /// DNS verification settings. Requires the `dns` feature.
    #[cfg(feature = "dns")]
    #[serde(default)]
    pub dns: crate::dns::DnsConfig,
    /// JWT signing and validation settings. Requires the `auth` feature.
    #[cfg(feature = "auth")]
    #[serde(default)]
    pub jwt: crate::auth::jwt::JwtConfig,
}
