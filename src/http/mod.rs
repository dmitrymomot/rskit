//! HTTP client with connection pooling, timeouts, and automatic retries.
//!
//! This module is gated behind the `http-client` feature flag.
//!
//! # Quick start
//!
//! 1. Enable the `http-client` feature in your `Cargo.toml`.
//! 2. Build a [`Client`] from [`ClientConfig`] or use [`Client::builder()`]
//!    for a fluent API.
//! 3. Use [`Client::get`], [`Client::post`], etc. to start a
//!    [`RequestBuilder`], then call [`RequestBuilder::send`] to dispatch.
//! 4. Read the [`Response`] body as JSON, text, bytes, or a streaming
//!    [`BodyStream`].
//!
//! # Key types
//!
//! - [`Client`] — reusable HTTP client; cheaply cloneable via `Arc`.
//! - [`ClientBuilder`] — fluent builder for constructing a [`Client`].
//! - [`ClientConfig`] — configuration (timeouts, retries, user-agent);
//!   deserializes from the `http` YAML section.
//! - [`RequestBuilder`] — per-request builder with headers, auth, body, and
//!   retry overrides.
//! - [`Response`] — received HTTP response with typed body consumption.
//! - [`BodyStream`] — streaming reader over a response body.
//!
//! # Example
//!
//! ```rust,ignore
//! use modo::http::{Client, ClientConfig};
//!
//! let client = Client::new(&ClientConfig::default());
//! let resp = client
//!     .get("https://api.example.com/data")
//!     .bearer_token("tok_abc")
//!     .send()
//!     .await?;
//! let body = resp.error_for_status()?.text().await?;
//! ```

mod client;
mod config;
mod request;
mod response;
mod retry;

pub use client::{Client, ClientBuilder};
pub use config::ClientConfig;
pub use request::RequestBuilder;
pub use response::{BodyStream, Response};
