use modo::AppConfig;
use modo_upload::UploadConfig;
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub(crate) struct Config {
    #[serde(flatten)]
    pub(crate) core: AppConfig,
    #[serde(default)]
    pub(crate) upload: UploadConfig,
}

#[cfg(feature = "sentry")]
impl modo::SentryConfigProvider for Config {
    fn sentry_config(&self) -> Option<&modo::SentryConfig> {
        self.core.sentry_config()
    }
}
