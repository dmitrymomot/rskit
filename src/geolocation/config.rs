use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct GeolocationConfig {
    pub mmdb_path: String,
}
