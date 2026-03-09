use crate::cors::CorsYamlConfig;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::fmt;

// ---------------------------------------------------------------------------
// HTTP config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    pub timeout: Option<u64>,
    pub body_limit: Option<String>,
    pub compression: bool,
    pub catch_panic: bool,
    pub trailing_slash: TrailingSlash,
    pub maintenance: bool,
    pub maintenance_message: Option<String>,
    pub sensitive_headers: bool,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            timeout: None,
            body_limit: None,
            compression: false,
            catch_panic: true,
            trailing_slash: TrailingSlash::default(),
            maintenance: false,
            maintenance_message: None,
            sensitive_headers: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrailingSlash {
    #[default]
    None,
    Strip,
    Add,
}

// ---------------------------------------------------------------------------
// Security headers config
// ---------------------------------------------------------------------------

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

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    pub requests: u32,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
    pub secret_key: String,
    pub log_level: String,
    pub trusted_proxies: Vec<String>,
    pub shutdown_timeout_secs: u64,
    pub cors: Option<CorsYamlConfig>,
    pub liveness_path: String,
    pub readiness_path: String,
    pub http: HttpConfig,
    pub security_headers: SecurityHeadersConfig,
    pub rate_limit: Option<RateLimitConfig>,
    #[cfg(any(feature = "static-fs", feature = "static-embed"))]
    pub static_files: Option<crate::static_files::StaticConfig>,
    #[serde(skip)]
    pub environment: Environment,
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
            cors: None,
            liveness_path: "/_live".to_string(),
            readiness_path: "/_ready".to_string(),
            http: HttpConfig::default(),
            security_headers: SecurityHeadersConfig::default(),
            rate_limit: None,
            #[cfg(any(feature = "static-fs", feature = "static-embed"))]
            static_files: None,
            environment: Environment::Development,
        }
    }
}

impl ServerConfig {
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
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
                result.push(bytes[i] as char);
                i += 1;
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

        result.push(bytes[i] as char);
        i += 1;
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
    let config_dir = "config";

    if !std::path::Path::new(config_dir).is_dir() {
        return Err(ConfigError::DirectoryNotFound {
            path: config_dir.to_string(),
        });
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
        unsafe { std::env::set_var("MODO_TEST_VAR", "hello") };
        assert_eq!(substitute_env_vars("${MODO_TEST_VAR}"), "hello");
        unsafe { std::env::remove_var("MODO_TEST_VAR") };
    }

    #[test]
    fn test_substitute_with_default() {
        unsafe { std::env::remove_var("MODO_UNSET_VAR") };
        assert_eq!(
            substitute_env_vars("${MODO_UNSET_VAR:-fallback}"),
            "fallback"
        );
    }

    #[test]
    fn test_substitute_empty_uses_default() {
        unsafe { std::env::set_var("MODO_EMPTY_VAR", "") };
        assert_eq!(
            substitute_env_vars("${MODO_EMPTY_VAR:-default_val}"),
            "default_val"
        );
        unsafe { std::env::remove_var("MODO_EMPTY_VAR") };
    }

    #[test]
    fn test_substitute_set_var_ignores_default() {
        unsafe { std::env::set_var("MODO_SET_VAR", "real") };
        assert_eq!(substitute_env_vars("${MODO_SET_VAR:-ignored}"), "real");
        unsafe { std::env::remove_var("MODO_SET_VAR") };
    }

    #[test]
    fn test_substitute_escaped() {
        assert_eq!(substitute_env_vars("\\${NOT_REPLACED}"), "${NOT_REPLACED}");
    }

    #[test]
    fn test_substitute_no_default_unset() {
        unsafe { std::env::remove_var("MODO_GONE_VAR") };
        assert_eq!(substitute_env_vars("port: ${MODO_GONE_VAR}"), "port: ");
    }

    #[test]
    fn test_substitute_invalid_var_name() {
        assert_eq!(substitute_env_vars("${invalid-name}"), "${invalid-name}");
    }

    #[test]
    fn test_substitute_mixed() {
        unsafe { std::env::set_var("MODO_MIX_A", "aaa") };
        unsafe { std::env::remove_var("MODO_MIX_B") };
        assert_eq!(
            substitute_env_vars("start-${MODO_MIX_A}-${MODO_MIX_B:-bbb}-end"),
            "start-aaa-bbb-end"
        );
        unsafe { std::env::remove_var("MODO_MIX_A") };
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
        assert!(cfg.http.body_limit.is_none());
        assert!(!cfg.http.compression);
        assert!(cfg.http.catch_panic);
        assert_eq!(cfg.http.trailing_slash, TrailingSlash::None);
        assert!(!cfg.http.maintenance);
        assert!(cfg.http.sensitive_headers);
        assert!(cfg.security_headers.enabled);
        assert!(cfg.rate_limit.is_none());
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
}
