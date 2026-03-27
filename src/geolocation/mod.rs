//! IP-to-location lookup using a MaxMind GeoLite2/GeoIP2 `.mmdb` database.
//!
//! This module is gated behind the `geolocation` feature flag.
//!
//! # Quick start
//!
//! 1. Add `geolocation` to your feature list and configure `mmdb_path` in your
//!    YAML config.
//! 2. Build a [`GeoLocator`] at startup with
//!    [`GeoLocator::from_config`] and register it in the service registry.
//! 3. Add [`ClientIpLayer`](crate::ip::ClientIpLayer) **before** [`GeoLayer`]
//!    in your middleware stack so client IP resolution happens first.
//! 4. Use [`Location`] as an axum extractor in handlers to read the resolved
//!    geolocation for each request.

mod config;
mod location;
mod locator;
mod middleware;

pub use config::GeolocationConfig;
pub use location::Location;
pub use locator::GeoLocator;
pub use middleware::GeoLayer;
