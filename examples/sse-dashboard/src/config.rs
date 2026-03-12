use modo::AppConfig;
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub(crate) struct Config {
    #[serde(flatten)]
    pub(crate) core: AppConfig,
}
