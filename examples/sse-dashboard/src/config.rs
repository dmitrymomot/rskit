use modo::AppConfig;
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub(crate) struct Config {
    #[serde(flatten)]
    pub(crate) core: AppConfig,
}

impl modo::SentryConfigProvider for Config {
    fn sentry_config(&self) -> Option<&modo::SentryConfig> {
        self.core.sentry_config()
    }
}
