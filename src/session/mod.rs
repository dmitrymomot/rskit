//! Database-backed HTTP session management.
//!
//! Sessions are stored in a SQLite table (`modo_sessions`) and identified by a
//! signed, opaque cookie. The middleware handles the full request/response
//! lifecycle: reading the session token from the cookie on the request path,
//! loading and fingerprint-validating the session, running the handler, and
//! then flushing dirty data or touching the expiry timestamp before writing the
//! `Set-Cookie` header on the response path.
//!
//! This module is always available — no feature flag required.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use modo::session::{self, SessionConfig, Store};
//! use modo::cookie::{CookieConfig, key_from_config};
//!
//! async fn build_app(
//!     pool: impl modo::db::Reader + modo::db::Writer,
//!     session_cfg: SessionConfig,
//!     cookie_cfg: CookieConfig,
//! ) -> modo::Result<axum::Router> {
//!     let key = key_from_config(&cookie_cfg)?;
//!     let store = Store::new(&pool, session_cfg);
//!     let session_layer = session::layer(store, &cookie_cfg, &key);
//!
//!     let router = axum::Router::new()
//!         // .route(...)
//!         .layer(session_layer);
//!
//!     Ok(router)
//! }
//! ```

mod config;
pub mod device;
mod extractor;
pub mod fingerprint;
pub mod meta;
mod middleware;
mod store;
mod token;

pub use config::SessionConfig;
pub use extractor::Session;
#[cfg(feature = "templates")]
pub(crate) use extractor::SessionState;
pub use middleware::SessionLayer;
pub use middleware::layer;
pub use store::{SessionData, Store};
pub use token::SessionToken;
