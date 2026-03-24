use serde::Deserialize;

/// Configuration for the geolocation module.
///
/// Deserializes from the `geolocation` section of the application YAML config.
/// Requires the `geolocation` feature.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct GeolocationConfig {
    /// Path to the MaxMind GeoLite2 or GeoIP2 `.mmdb` database file.
    ///
    /// Supports `${VAR}` and `${VAR:default}` env-var substitution when loaded
    /// through the framework's config loader. An empty path causes
    /// [`GeoLocator::from_config`](super::GeoLocator::from_config) to return an error.
    pub mmdb_path: String,
}
