use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    // server field added in Task 12 when server::Config exists
    pub database: crate::db::Config,
    pub tracing: crate::tracing::Config,
}
