use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub server: crate::server::Config,
    pub database: crate::db::Config,
    pub tracing: crate::tracing::Config,
}
