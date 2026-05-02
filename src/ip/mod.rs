//! # modo::ip
//!
//! Client IP extraction with trusted proxy support.
//!
//! Provides:
//! - [`ClientIp`] — axum extractor wrapping `std::net::IpAddr`
//! - [`ClientIpLayer`] — Tower layer that resolves the client IP and inserts
//!   [`ClientIp`] into request extensions
//! - [`extract_client_ip`] — low-level resolution function (headers + trusted
//!   proxies + fallback)
//!
//! For the richer [`ClientInfo`](crate::client::ClientInfo) extractor (with
//! parsed device fields and a server-computed fingerprint), see
//! [`crate::client`].
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use axum::{Router, routing::get};
//! use modo::ip::{ClientIp, ClientIpLayer};
//!
//! let app: Router = Router::new()
//!     .route("/", get(handler))
//!     .layer(ClientIpLayer::new());
//!
//! async fn handler(ClientIp(ip): ClientIp) -> String {
//!     ip.to_string()
//! }
//! ```

mod client_ip;
mod extract;
mod middleware;

pub use client_ip::ClientIp;
pub use extract::extract_client_ip;
pub use middleware::ClientIpLayer;
