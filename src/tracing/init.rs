use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub level: String,
    pub format: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
        }
    }
}

pub fn init(config: &Config) -> crate::error::Result<()> {
    use tracing_subscriber::{EnvFilter, fmt};

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    match config.format.as_str() {
        "json" => {
            fmt().json().with_env_filter(filter).try_init().ok();
        }
        "pretty" => {
            fmt().pretty().with_env_filter(filter).try_init().ok();
        }
        _ => {
            fmt().with_env_filter(filter).try_init().ok();
        }
    }
    Ok(())
}
