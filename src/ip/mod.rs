//! Client IP extraction with trusted proxy support.
//!
//! Provides the [`ClientIp`] extractor for reading the real client IP from
//! incoming requests, respecting `X-Forwarded-For` headers when the request
//! arrives through a trusted reverse proxy.
//!
//! [`ClientIpLayer`] is middleware that records the client IP in request
//! extensions so downstream handlers and middleware can access it.

mod client_ip;
mod extract;
mod middleware;

pub use client_ip::ClientIp;
pub use extract::extract_client_ip;
pub use middleware::ClientIpLayer;
