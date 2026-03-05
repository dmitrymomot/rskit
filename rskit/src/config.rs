use std::env;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind_address: String,
    pub database_url: String,
    pub secret_key: String,
    pub environment: Environment,
    pub log_level: String,
    pub sentry_dsn: Option<String>,
    pub sentry_log_level: String,
    pub session_ttl: Duration,
    pub session_cookie_name: String,
    pub session_validate_fingerprint: bool,
    pub session_touch_interval: Duration,
    pub trusted_proxies: Vec<ipnet::IpNet>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    Development,
    Production,
    Test,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:3000".to_string(),
            database_url: "sqlite://data.db?mode=rwc".to_string(),
            secret_key: String::new(),
            environment: Environment::Development,
            log_level: "info".to_string(),
            sentry_dsn: None,
            sentry_log_level: "error".to_string(),
            session_ttl: Duration::from_secs(30 * 24 * 60 * 60), // 30 days
            session_cookie_name: "_rskit_session".to_string(),
            session_validate_fingerprint: true,
            session_touch_interval: Duration::from_secs(5 * 60), // 5 minutes
            trusted_proxies: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();

        let environment = match env::var("RSKIT_ENV")
            .unwrap_or_else(|_| "development".to_string())
            .to_lowercase()
            .as_str()
        {
            "production" | "prod" => Environment::Production,
            "test" => Environment::Test,
            _ => Environment::Development,
        };

        Self {
            bind_address: env::var("RSKIT_BIND_ADDRESS")
                .unwrap_or_else(|_| "0.0.0.0:3000".to_string()),
            database_url: env::var("RSKIT_DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://data.db?mode=rwc".to_string()),
            secret_key: env::var("RSKIT_SECRET_KEY").unwrap_or_default(),
            environment,
            log_level: env::var("RSKIT_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            sentry_dsn: env::var("RSKIT_SENTRY_DSN").ok().filter(|s| !s.is_empty()),
            sentry_log_level: env::var("RSKIT_SENTRY_LOG_LEVEL")
                .unwrap_or_else(|_| "error".to_string()),
            session_ttl: Duration::from_secs({
                let default = 30 * 24 * 60 * 60;
                match env::var("RSKIT_SESSION_TTL") {
                    Ok(v) => v.parse().unwrap_or_else(|e| {
                        tracing::warn!("Invalid RSKIT_SESSION_TTL='{v}': {e}, using default");
                        default
                    }),
                    Err(_) => default,
                }
            }),
            session_cookie_name: env::var("RSKIT_SESSION_COOKIE_NAME")
                .unwrap_or_else(|_| "_rskit_session".to_string()),
            session_validate_fingerprint: env::var("RSKIT_SESSION_VALIDATE_FINGERPRINT")
                .map(|v| v != "false" && v != "0")
                .unwrap_or(true),
            session_touch_interval: Duration::from_secs({
                let default = 5 * 60;
                match env::var("RSKIT_SESSION_TOUCH_INTERVAL") {
                    Ok(v) => v.parse().unwrap_or_else(|e| {
                        tracing::warn!(
                            "Invalid RSKIT_SESSION_TOUCH_INTERVAL='{v}': {e}, using default"
                        );
                        default
                    }),
                    Err(_) => default,
                }
            }),
            trusted_proxies: env::var("RSKIT_TRUSTED_PROXIES")
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .filter_map(|s| {
                    let s = s.trim();
                    s.parse::<ipnet::IpNet>()
                        .or_else(|_| s.parse::<std::net::IpAddr>().map(ipnet::IpNet::from))
                        .map_err(|e| {
                            tracing::warn!("Ignoring invalid trusted_proxies entry '{s}': {e}");
                        })
                        .ok()
                })
                .collect(),
        }
    }
}
