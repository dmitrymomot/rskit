use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub server: crate::server::Config,
    pub database: crate::db::Config,
    pub tracing: crate::tracing::Config,
    pub cookie: Option<crate::cookie::CookieConfig>,
    pub security_headers: crate::middleware::SecurityHeadersConfig,
    pub cors: crate::middleware::CorsConfig,
    pub csrf: crate::middleware::CsrfConfig,
    pub rate_limit: crate::middleware::RateLimitConfig,
    pub session: crate::session::SessionConfig,
    pub job: crate::job::JobConfig,
    #[cfg(feature = "auth")]
    #[serde(default)]
    pub oauth: crate::auth::oauth::OAuthConfig,
    #[cfg(feature = "email")]
    #[serde(default)]
    pub email: crate::email::EmailConfig,
    #[cfg(feature = "templates")]
    #[serde(default)]
    pub template: crate::template::TemplateConfig,
}
