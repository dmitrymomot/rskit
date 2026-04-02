//! Outbound webhook delivery following the
//! [Standard Webhooks](https://www.standardwebhooks.com/) specification.
//!
//! This module provides signed outbound HTTP POST requests using HMAC-SHA256,
//! plus verification helpers for incoming webhook requests.
//!
//! Requires the `webhooks` feature flag:
//!
//! ```toml
//! [dependencies]
//! modo = { version = "0.5", features = ["webhooks"] }
//! ```
//!
//! # Provides
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`WebhookSender`] | Signs and delivers webhook payloads via HTTP POST. Clone-cheap (`Arc` inside). |
//! | [`WebhookSecret`] | HMAC-SHA256 signing key. Serialized as `whsec_<base64>`. `Debug` output is redacted. |
//! | [`WebhookResponse`] | HTTP status code and body bytes returned by the endpoint. |
//! | [`SignedHeaders`] | The three Standard Webhooks request headers produced by [`sign_headers()`]. |
//! | [`sign()`] | Compute a raw HMAC-SHA256 signature (base64-encoded). |
//! | [`verify()`] | Verify a raw HMAC-SHA256 signature with constant-time comparison. |
//! | [`sign_headers()`] | Build the three Standard Webhooks headers from id, timestamp, body, and secrets. |
//! | [`verify_headers()`] | Verify incoming Standard Webhooks headers with replay-attack protection. |
//!
//! # Quick start
//!
//! ```
//! use modo::webhook::{WebhookSender, WebhookSecret};
//!
//! # async fn example() -> modo::Result<()> {
//! let sender = WebhookSender::default_client();
//! let secret: WebhookSecret = "whsec_dGVzdC1rZXktYnl0ZXM=".parse()?;
//!
//! let response = sender.send(
//!     "https://example.com/webhooks",
//!     "msg_01HXYZ",
//!     b"{\"event\":\"user.created\"}",
//!     &[&secret],
//! ).await?;
//!
//! println!("endpoint returned {}", response.status);
//! # Ok(())
//! # }
//! ```

mod client;
mod secret;
mod sender;
mod signature;

pub use client::WebhookResponse;
pub use secret::WebhookSecret;
pub use sender::WebhookSender;
pub use signature::{SignedHeaders, sign, sign_headers, verify, verify_headers};
