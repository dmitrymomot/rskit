use modo_upload::UploadConfig;
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub(crate) struct Config {
    #[serde(flatten)]
    pub(crate) core: modo::config::AppConfig,
    #[serde(default)]
    pub(crate) upload: UploadConfig,
}
