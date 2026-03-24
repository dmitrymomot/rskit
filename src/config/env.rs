use std::env as std_env;

const APP_ENV_KEY: &str = "APP_ENV";
const DEFAULT_ENV: &str = "development";

/// Returns the current application environment.
///
/// Reads the `APP_ENV` environment variable. Falls back to `"development"` when
/// the variable is not set.
pub fn env() -> String {
    std_env::var(APP_ENV_KEY).unwrap_or_else(|_| DEFAULT_ENV.to_string())
}

/// Returns `true` when `APP_ENV` is `"development"` (or unset).
pub fn is_dev() -> bool {
    env() == "development"
}

/// Returns `true` when `APP_ENV` is `"production"`.
pub fn is_prod() -> bool {
    env() == "production"
}

/// Returns `true` when `APP_ENV` is `"test"`.
pub fn is_test() -> bool {
    env() == "test"
}
