use modo::AppConfig;
use modo_db::DatabaseConfig;
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub(crate) struct Config {
    #[serde(flatten)]
    pub(crate) core: AppConfig,
    pub(crate) database: DatabaseConfig,
}

impl modo::SentryConfigProvider for Config {
    fn sentry_config(&self) -> Option<&modo::SentryConfig> {
        self.core.sentry_config()
    }
}
