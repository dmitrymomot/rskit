use serde::de::DeserializeOwned;
use std::path::Path;

use super::env::env;
use super::substitute::substitute_env_vars;
use crate::error::Result;

/// Loads and deserializes a YAML config file for the current environment.
///
/// The file is resolved as `{config_dir}/{APP_ENV}.yaml`, where `APP_ENV` is
/// read from the process environment via [`super::env()`] (defaults to
/// `"development"`). After reading, all `${VAR}` and `${VAR:default}`
/// placeholders are replaced with values from the process environment via
/// [`substitute_env_vars`] before YAML deserialization with `serde_yaml_ng`.
///
/// `T` may be [`crate::Config`] or any application-specific struct — flatten
/// `Config` with `#[serde(flatten)]` to keep the framework fields alongside
/// custom ones.
///
/// # Errors
///
/// Returns [`crate::Error`] when:
/// - The config file cannot be read (missing file, permission denied, …).
/// - A required `${VAR}` placeholder references an unset environment variable
///   and provides no default.
/// - A `${...` placeholder is unclosed.
/// - The YAML cannot be deserialized into `T`.
///
/// # Example
///
/// ```no_run
/// use modo::config::load;
/// use modo::Config;
///
/// let config: Config = load("config/").unwrap();
/// ```
pub fn load<T: DeserializeOwned>(config_dir: &str) -> Result<T> {
    let environment = env();
    let file_path = Path::new(config_dir).join(format!("{environment}.yaml"));

    let raw = std::fs::read_to_string(&file_path).map_err(|e| {
        crate::error::Error::internal(format!(
            "failed to read config file '{}': {e}",
            file_path.display()
        ))
    })?;

    let substituted = substitute_env_vars(&raw)?;
    let config: T = serde_yaml_ng::from_str(&substituted)?;
    Ok(config)
}
