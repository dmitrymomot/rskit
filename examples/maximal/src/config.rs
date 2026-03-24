use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(flatten)]
    pub modo: modo::Config,
    pub job_database: modo::db::Config,
}
