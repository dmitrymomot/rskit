//! # modo::geolocation
//!
//! IP-to-location lookup using a MaxMind GeoLite2/GeoIP2 `.mmdb` database.
//!
//! Provides:
//!
//! - [`GeolocationConfig`] — YAML-deserializable config with `mmdb_path`
//! - [`GeoLocator`] — MaxMind database reader; cheaply cloneable via `Arc`
//! - [`GeoLayer`] — Tower layer that resolves location per request; also
//!   re-exported as [`modo::middlewares::Geo`](crate::middlewares::Geo)
//! - [`Location`] — resolved geolocation data; doubles as an axum extractor
//!
//! Requires a MaxMind GeoLite2-City or GeoIP2-City `.mmdb` database file;
//! the framework does not ship one. See the module `README.md` for setup.
//!
//! ## Quick start
//!
//! 1. Configure `mmdb_path` in the `geolocation` section of your YAML config.
//! 2. Build a [`GeoLocator`] at startup with [`GeoLocator::from_config`].
//! 3. Add [`ClientIpLayer`](crate::ip::ClientIpLayer) **before** [`GeoLayer`]
//!    in your middleware stack so client IP resolution happens first.
//! 4. Use [`Location`] as an axum extractor in handlers to read the resolved
//!    geolocation for each request.
//!
//! ```rust,no_run
//! use axum::{Router, routing::get};
//! use modo::geolocation::{GeoLayer, GeoLocator, GeolocationConfig, Location};
//! use modo::ip::ClientIpLayer;
//!
//! # fn main() -> modo::Result<()> {
//! let mut config = GeolocationConfig::default();
//! config.mmdb_path = "data/GeoLite2-City.mmdb".into();
//! let locator = GeoLocator::from_config(&config)?;
//!
//! async fn handler(location: Location) -> String {
//!     format!("country: {:?}", location.country_code)
//! }
//!
//! let app: Router = Router::new()
//!     .route("/", get(handler))
//!     .layer(GeoLayer::new(locator))
//!     .layer(ClientIpLayer::new());
//! # Ok(())
//! # }
//! ```

mod config;
mod location;
mod locator;
mod middleware;

pub use config::GeolocationConfig;
pub use location::Location;
pub use locator::GeoLocator;
pub use middleware::GeoLayer;
