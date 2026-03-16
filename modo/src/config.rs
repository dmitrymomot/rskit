use crate::cors::CorsYamlConfig;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::fmt;

// ---------------------------------------------------------------------------
// HTTP config
// ---------------------------------------------------------------------------

/// HTTP-level middleware settings, configurable under `server.http` in YAML.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    /// Request timeout in seconds. `None` disables the timeout.
    pub timeout: Option<u64>,
    /// Maximum request body size (e.g. `"2mb"`, `"512kb"`). `None` means unlimited.
    pub body_limit: Option<String>,
    /// Enable response compression via `CompressionLayer`.
    pub compression: bool,
    /// Enable the catch-panic middleware (converts panics into 500 responses).
    pub catch_panic: bool,
    /// How to handle trailing slashes in request paths.
    pub trailing_slash: TrailingSlash,
    /// Enable maintenance mode (returns 503 for all requests).
    pub maintenance: bool,
    /// Optional custom message returned in maintenance mode responses.
    pub maintenance_message: Option<String>,
    /// Redact `Authorization`, `Cookie`, `Set-Cookie`, and `Proxy-Authorization` headers from logs.
    pub sensitive_headers: bool,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            timeout: None,
            body_limit: Some("2mb".to_string()),
            compression: false,
            catch_panic: true,
            trailing_slash: TrailingSlash::default(),
            maintenance: false,
            maintenance_message: None,
            sensitive_headers: true,
        }
    }
}

/// Controls how trailing slashes in request paths are handled.
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrailingSlash {
    /// No modification (default).
    #[default]
    None,
    /// Redirect requests with trailing slashes to the non-trailing-slash URL.
    Strip,
    /// Redirect requests without trailing slashes to the trailing-slash URL.
    Add,
}

// ---------------------------------------------------------------------------
// Security headers config
// ---------------------------------------------------------------------------

/// Configuration for security-related HTTP response headers.
///
/// Applied by the security headers middleware when `enabled` is `true`.
/// Defaults enable HSTS, `X-Content-Type-Options`, `X-Frame-Options`,
/// `Referrer-Policy`, `Permissions-Policy`, and a restrictive `CSP`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SecurityHeadersConfig {
    pub enabled: bool,
    pub x_content_type_options: Option<String>,
    pub x_frame_options: Option<String>,
    pub referrer_policy: Option<String>,
    pub permissions_policy: Option<String>,
    pub content_security_policy: Option<String>,
    pub hsts: bool,
    pub hsts_max_age: u64,
}

impl Default for SecurityHeadersConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            x_content_type_options: Some("nosniff".to_string()),
            x_frame_options: Some("DENY".to_string()),
            referrer_policy: Some("strict-origin-when-cross-origin".to_string()),
            permissions_policy: Some("camera=(), microphone=(), geolocation=()".to_string()),
            content_security_policy: Some("default-src 'self'".to_string()),
            hsts: true,
            hsts_max_age: 31_536_000,
        }
    }
}

// ---------------------------------------------------------------------------
// Rate limit config
// ---------------------------------------------------------------------------

/// Token-bucket rate limiting configuration, applied globally by IP.
///
/// Configured under `server.rate_limit` in YAML, or via `AppBuilder::rate_limit`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    /// Maximum number of requests allowed per window.
    pub requests: u32,
    /// Window duration in seconds.
    pub window_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests: 100,
            window_secs: 60,
        }
    }
}

// ---------------------------------------------------------------------------
// Size parsing utility
// ---------------------------------------------------------------------------

/// Parse a human-readable size string into bytes.
/// Supports: "100", "2kb", "2KB", "2mb", "1gb".
pub fn parse_size(s: &str) -> Result<usize, String> {
    let s = s.trim().to_lowercase();
    let (num_str, multiplier) = if let Some(n) = s.strip_suffix("gb") {
        (n, 1024 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("mb") {
        (n, 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("kb") {
        (n, 1024)
    } else if let Some(n) = s.strip_suffix('b') {
        (n, 1)
    } else {
        (s.as_str(), 1)
    };

    let num: usize = num_str
        .trim()
        .parse()
        .map_err(|_| format!("invalid size: {s}"))?;
    Ok(num * multiplier)
}

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

/// The runtime environment, detected from the `MODO_ENV` environment variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    Development,
    Production,
    Test,
    Custom(String),
}

impl From<&str> for Environment {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "development" | "dev" => Self::Development,
            "production" | "prod" => Self::Production,
            "test" => Self::Test,
            other => Self::Custom(other.to_string()),
        }
    }
}

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Environment {
    /// Return the string representation of the environment.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
            Self::Test => "test",
            Self::Custom(s) => s,
        }
    }
}

// ---------------------------------------------------------------------------
// ServerConfig
// ---------------------------------------------------------------------------

/// Low-level server configuration, deserialized from the `server` key in YAML.
///
/// Most fields have sensible defaults. `secret_key` must be set in production
/// for stable cookie signing/encryption across restarts.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// TCP port to listen on. Default: `3000`.
    pub port: u16,
    /// Bind address. Default: `"0.0.0.0"`.
    pub host: String,
    /// Secret key used to sign and encrypt cookies. Empty = random key per restart.
    pub secret_key: String,
    /// Log level filter (trace/debug/info/warn/error). Default: `"info"`.
    pub log_level: String,
    /// Trusted proxy CIDR ranges for client IP extraction (e.g. `["10.0.0.0/8"]`).
    pub trusted_proxies: Vec<String>,
    /// Graceful shutdown timeout in seconds. Default: `30`.
    pub shutdown_timeout_secs: u64,
    /// Per-hook timeout in seconds during graceful shutdown. Default: `5`.
    pub hook_timeout_secs: u64,
    /// Optional CORS policy loaded from YAML (`server.cors`).
    pub cors: Option<CorsYamlConfig>,
    /// Path for the liveness health check endpoint. Default: `"/_live"`.
    pub liveness_path: String,
    /// Path for the readiness health check endpoint. Default: `"/_ready"`.
    pub readiness_path: String,
    pub http: HttpConfig,
    pub security_headers: SecurityHeadersConfig,
    /// Global rate limit policy. `None` disables rate limiting.
    pub rate_limit: Option<RateLimitConfig>,
    #[cfg(any(feature = "static-fs", feature = "static-embed"))]
    pub static_files: Option<crate::static_files::StaticConfig>,
    /// Show the startup banner with version, environment, and route info. Default: `true`.
    #[serde(default = "default_true")]
    pub show_banner: bool,
    #[serde(skip)]
    pub environment: Environment,
}

fn default_true() -> bool {
    true
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 3000,
            host: "0.0.0.0".to_string(),
            secret_key: String::new(),
            log_level: "info".to_string(),
            trusted_proxies: Vec::new(),
            shutdown_timeout_secs: 30,
            hook_timeout_secs: 5,
            cors: None,
            liveness_path: "/_live".to_string(),
            readiness_path: "/_ready".to_string(),
            http: HttpConfig::default(),
            security_headers: SecurityHeadersConfig::default(),
            rate_limit: None,
            #[cfg(any(feature = "static-fs", feature = "static-embed"))]
            static_files: None,
            show_banner: true,
            environment: Environment::Development,
        }
    }
}

impl ServerConfig {
    /// Return `"host:port"` as a string suitable for binding a TCP listener.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

// ---------------------------------------------------------------------------
// AppConfig
// ---------------------------------------------------------------------------

/// Unified application configuration.
///
/// Top-level config type for `#[modo::main]`. Includes server settings
/// and optional feature-specific sections (cookies, templates, i18n, CSRF).
/// All feature sections use `#[serde(default)]` — absent in YAML means defaults apply.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub cookies: crate::cookies::CookieConfig,
    #[cfg(feature = "templates")]
    pub templates: crate::templates::TemplateConfig,
    #[cfg(feature = "i18n")]
    pub i18n: crate::i18n::I18nConfig,
    #[cfg(feature = "csrf")]
    pub csrf: crate::csrf::CsrfConfig,
    #[cfg(feature = "sse")]
    #[serde(default)]
    pub sse: crate::sse::SseConfig,
}

// ---------------------------------------------------------------------------
// ConfigError
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file '{path}': {source}")]
    FileRead {
        path: String,
        source: std::io::Error,
    },

    #[error("failed to parse config file '{path}': {source}")]
    Parse {
        path: String,
        source: serde_yaml_ng::Error,
    },

    #[error("config directory not found: '{path}'")]
    DirectoryNotFound { path: String },
}

// ---------------------------------------------------------------------------
// Environment detection
// ---------------------------------------------------------------------------

/// Detect the runtime environment from the `MODO_ENV` environment variable.
///
/// Defaults to `"development"` when the variable is not set.
pub fn detect_env() -> Environment {
    std::env::var("MODO_ENV")
        .unwrap_or_else(|_| "development".to_string())
        .as_str()
        .into()
}

// ---------------------------------------------------------------------------
// Env var substitution
// ---------------------------------------------------------------------------

/// Substitute `${VAR}` and `${VAR:-default}` patterns in the input string.
///
/// - `${VAR}` is replaced with the env var value, or empty string if unset.
/// - `${VAR:-default}` is replaced with the env var value, or `default` if unset/empty.
/// - `\${VAR}` is escaped, producing literal `${VAR}`.
/// - Invalid var names (empty, contains hyphens) are passed through literally.
/// - No nested `${...}`, no multiline defaults.
pub fn substitute_env_vars(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Check for escaped `\${`
        if i + 2 < len && bytes[i] == b'\\' && bytes[i + 1] == b'$' && bytes[i + 2] == b'{' {
            // Find the closing brace to output the literal `${...}`
            if let Some(close) = find_closing_brace(input, i + 3) {
                result.push_str(&input[i + 1..=close]);
                i = close + 1;
            } else {
                let ch = input[i..].chars().next().unwrap();
                result.push(ch);
                i += ch.len_utf8();
            }
            continue;
        }

        // Check for `${`
        if i + 1 < len
            && bytes[i] == b'$'
            && bytes[i + 1] == b'{'
            && let Some(close) = find_closing_brace(input, i + 2)
        {
            let inner = &input[i + 2..close];

            // Split on `:-` for default value
            let (var_name, default) = if let Some(sep) = inner.find(":-") {
                (&inner[..sep], Some(&inner[sep + 2..]))
            } else {
                (inner, None)
            };

            if is_valid_var_name(var_name) {
                match std::env::var(var_name) {
                    Ok(val) if !val.is_empty() => result.push_str(&val),
                    _ => {
                        if let Some(def) = default {
                            result.push_str(def);
                        }
                    }
                }
                i = close + 1;
            } else {
                // Invalid var name — pass through literally
                result.push_str(&input[i..=close]);
                i = close + 1;
            }
            continue;
        }

        let ch = input[i..].chars().next().unwrap();
        result.push(ch);
        i += ch.len_utf8();
    }

    result
}

fn find_closing_brace(input: &str, start: usize) -> Option<usize> {
    input[start..].find('}').map(|pos| start + pos)
}

fn is_valid_var_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

// ---------------------------------------------------------------------------
// Config loaders
// ---------------------------------------------------------------------------

/// Load config: dotenv + detect env + read YAML + substitute + deserialize.
pub fn load<T: DeserializeOwned>() -> Result<T, ConfigError> {
    let _ = dotenvy::dotenv();
    let env = detect_env();
    load_for_env(env.as_str())
}

/// Load config for an explicit environment name.
pub fn load_for_env<T: DeserializeOwned>(env: &str) -> Result<T, ConfigError> {
    let config_dir = std::env::var("MODO_CONFIG_DIR").unwrap_or_else(|_| "config".to_string());

    if !std::path::Path::new(&config_dir).is_dir() {
        return Err(ConfigError::DirectoryNotFound { path: config_dir });
    }

    let path = format!("{config_dir}/{env}.yaml");
    let raw = std::fs::read_to_string(&path).map_err(|e| ConfigError::FileRead {
        path: path.clone(),
        source: e,
    })?;

    let substituted = substitute_env_vars(&raw);

    serde_yaml_ng::from_str(&substituted).map_err(|e| ConfigError::Parse { path, source: e })
}

/// Load config, falling back to `T::default()` if the config directory or file is missing.
pub fn load_or_default<T: DeserializeOwned + Default>() -> Result<T, ConfigError> {
    match load::<T>() {
        Ok(cfg) => Ok(cfg),
        Err(ConfigError::DirectoryNotFound { .. }) | Err(ConfigError::FileRead { .. }) => {
            Ok(T::default())
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_simple_var() {
        temp_env::with_var("MODO_TEST_VAR", Some("hello"), || {
            assert_eq!(substitute_env_vars("${MODO_TEST_VAR}"), "hello");
        });
    }

    #[test]
    fn test_substitute_with_default() {
        temp_env::with_var("MODO_UNSET_VAR", None::<&str>, || {
            assert_eq!(
                substitute_env_vars("${MODO_UNSET_VAR:-fallback}"),
                "fallback"
            );
        });
    }

    #[test]
    fn test_substitute_empty_uses_default() {
        temp_env::with_var("MODO_EMPTY_VAR", Some(""), || {
            assert_eq!(
                substitute_env_vars("${MODO_EMPTY_VAR:-default_val}"),
                "default_val"
            );
        });
    }

    #[test]
    fn test_substitute_set_var_ignores_default() {
        temp_env::with_var("MODO_SET_VAR", Some("real"), || {
            assert_eq!(substitute_env_vars("${MODO_SET_VAR:-ignored}"), "real");
        });
    }

    #[test]
    fn test_substitute_escaped() {
        assert_eq!(substitute_env_vars("\\${NOT_REPLACED}"), "${NOT_REPLACED}");
    }

    #[test]
    fn test_substitute_no_default_unset() {
        temp_env::with_var("MODO_GONE_VAR", None::<&str>, || {
            assert_eq!(substitute_env_vars("port: ${MODO_GONE_VAR}"), "port: ");
        });
    }

    #[test]
    fn test_substitute_invalid_var_name() {
        assert_eq!(substitute_env_vars("${invalid-name}"), "${invalid-name}");
    }

    #[test]
    fn test_substitute_mixed() {
        temp_env::with_vars([("MODO_MIX_A", Some("aaa")), ("MODO_MIX_B", None)], || {
            assert_eq!(
                substitute_env_vars("start-${MODO_MIX_A}-${MODO_MIX_B:-bbb}-end"),
                "start-aaa-bbb-end"
            );
        });
    }

    #[test]
    fn test_substitute_preserves_non_ascii() {
        assert_eq!(substitute_env_vars("name: José"), "name: José");
        assert_eq!(substitute_env_vars("emoji: 🚀 done"), "emoji: 🚀 done");
    }

    #[test]
    fn test_environment_from_str() {
        assert_eq!(Environment::from("development"), Environment::Development);
        assert_eq!(Environment::from("dev"), Environment::Development);
        assert_eq!(Environment::from("production"), Environment::Production);
        assert_eq!(Environment::from("prod"), Environment::Production);
        assert_eq!(Environment::from("test"), Environment::Test);
        assert_eq!(
            Environment::from("staging"),
            Environment::Custom("staging".to_string())
        );
    }

    #[test]
    fn test_environment_display() {
        assert_eq!(Environment::Development.to_string(), "development");
        assert_eq!(Environment::Production.to_string(), "production");
        assert_eq!(Environment::Test.to_string(), "test");
        assert_eq!(
            Environment::Custom("staging".to_string()).to_string(),
            "staging"
        );
    }

    #[test]
    fn test_server_config_defaults() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.port, 3000);
        assert_eq!(cfg.host, "0.0.0.0");
        assert_eq!(cfg.secret_key, "");
        assert_eq!(cfg.log_level, "info");
        assert!(cfg.trusted_proxies.is_empty());
        assert_eq!(cfg.shutdown_timeout_secs, 30);
        assert!(cfg.cors.is_none());
        assert_eq!(cfg.liveness_path, "/_live");
        assert_eq!(cfg.readiness_path, "/_ready");
        // New middleware config defaults
        assert!(cfg.http.timeout.is_none());
        assert_eq!(cfg.http.body_limit, Some("2mb".to_string()));
        assert!(!cfg.http.compression);
        assert!(cfg.http.catch_panic);
        assert_eq!(cfg.http.trailing_slash, TrailingSlash::None);
        assert!(!cfg.http.maintenance);
        assert!(cfg.http.sensitive_headers);
        assert!(cfg.security_headers.enabled);
        assert!(cfg.rate_limit.is_none());
    }

    #[test]
    fn test_server_config_hook_timeout_default() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.hook_timeout_secs, 5);
    }

    #[test]
    fn test_server_config_hook_timeout_yaml() {
        let yaml = "server:\n  hook_timeout_secs: 15\n";
        let cfg: AppConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(cfg.server.hook_timeout_secs, 15);
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("100"), Ok(100));
        assert_eq!(parse_size("2kb"), Ok(2048));
        assert_eq!(parse_size("2KB"), Ok(2048));
        assert_eq!(parse_size("2mb"), Ok(2 * 1024 * 1024));
        assert_eq!(parse_size("1gb"), Ok(1024 * 1024 * 1024));
        assert_eq!(parse_size("512b"), Ok(512));
        assert!(parse_size("abc").is_err());
    }

    #[test]
    fn test_server_config_bind_address() {
        let cfg = ServerConfig {
            port: 8080,
            host: "127.0.0.1".to_string(),
            ..Default::default()
        };
        assert_eq!(cfg.bind_address(), "127.0.0.1:8080");
    }

    #[test]
    fn test_app_config_defaults() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.server.port, 3000);
        assert_eq!(cfg.cookies.path, "/");
    }

    #[test]
    fn test_app_config_yaml_minimal() {
        let yaml = "server:\n  port: 8080\n";
        let cfg: AppConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(cfg.server.port, 8080);
        // cookies get defaults
        assert_eq!(cfg.cookies.path, "/");
    }

    #[test]
    fn test_config_dir_from_env_var() {
        let dir = std::env::temp_dir().join("modo_config_dir_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("test.yaml"), "server:\n  port: 9999\n").unwrap();

        let cfg: AppConfig =
            temp_env::with_var("MODO_CONFIG_DIR", Some(dir.to_str().unwrap()), || {
                load_for_env("test").unwrap()
            });

        assert_eq!(cfg.server.port, 9999);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_config_dir_defaults_to_config() {
        temp_env::with_var("MODO_CONFIG_DIR", None::<&str>, || {
            let result: Result<AppConfig, _> = load_for_env("nonexistent_env_12345");
            match result {
                Err(ConfigError::DirectoryNotFound { path })
                | Err(ConfigError::FileRead { path, .. }) => {
                    assert!(
                        path.starts_with("config"),
                        "expected config dir path, got: {path}"
                    );
                }
                _ => {
                    // If ./config dir exists with the file, that's also fine
                }
            }
        });
    }
}
