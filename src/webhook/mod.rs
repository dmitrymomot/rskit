//! Outbound webhook delivery following the Standard Webhooks specification.
//!
//! This module provides signed outbound HTTP POST requests using HMAC-SHA256.
//! All types require the `"webhooks"` feature.

mod client;
mod secret;
mod sender;
mod signature;

pub use client::WebhookResponse;
pub use secret::WebhookSecret;
pub use sender::WebhookSender;
pub use signature::{SignedHeaders, sign, sign_headers, verify, verify_headers};
