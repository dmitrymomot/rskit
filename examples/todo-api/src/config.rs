use modo_db::DatabaseConfig;
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub(crate) struct Config {
    #[serde(flatten)]
    pub(crate) core: modo::config::AppConfig,
    pub(crate) database: DatabaseConfig,
}
