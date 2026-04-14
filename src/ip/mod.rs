//! # modo::ip
//!
//! Client IP extraction with trusted proxy support.
//!
//! Provides:
//! - [`ClientIp`] — axum extractor wrapping `std::net::IpAddr`
//! - [`ClientInfo`] — structured client metadata (IP, user-agent, etc.) inserted
//!   into request extensions by [`ClientIpLayer`]
//! - [`ClientIpLayer`] — Tower layer that resolves the client IP and inserts
//!   [`ClientIp`] / [`ClientInfo`] into request extensions
//! - [`extract_client_ip`] — low-level resolution function (headers + trusted
//!   proxies + fallback)
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

mod client_info;
mod client_ip;
mod extract;
mod middleware;

pub use client_info::ClientInfo;
pub use client_ip::ClientIp;
pub use extract::extract_client_ip;
pub use middleware::ClientIpLayer;
