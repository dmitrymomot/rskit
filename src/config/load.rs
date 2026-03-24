use serde::de::DeserializeOwned;
use std::path::Path;

use super::env::env;
use super::substitute::substitute_env_vars;
use crate::error::Result;

/// Loads and deserializes a YAML config file for the current environment.
///
/// The file is resolved as `{config_dir}/{APP_ENV}.yaml`. After reading, all
/// `${VAR}` and `${VAR:default}` placeholders are replaced with values from
/// the process environment before deserialization.
///
/// # Errors
///
/// Returns an error when:
/// - The config file cannot be read.
/// - A required `${VAR}` placeholder references an unset environment variable.
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
