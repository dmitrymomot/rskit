//! # modo::client
//!
//! Client-context types and helpers shared across HTTP, audit, and session
//! code paths.
//!
//! Provides:
//!
//! - [`ClientInfo`] — structured client metadata (IP, user-agent, parsed
//!   device fields, server-computed fingerprint). Populated automatically
//!   in handlers via [`FromRequestParts`](axum::extract::FromRequestParts),
//!   or built manually for non-HTTP contexts (background jobs, CLI tools).
//! - [`parse_device_name`] / [`parse_device_type`] — `User-Agent` classifiers.
//! - [`compute_fingerprint`] — SHA-256 hash of UA + Accept-Language +
//!   Accept-Encoding for session-hijack detection.
//!
//! Used by [`crate::audit`] (persisted with audit events) and
//! [`crate::auth::session`] (session creation, fingerprint validation).
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use modo::client::ClientInfo;
//!
//! async fn handler(info: ClientInfo) -> String {
//!     format!(
//!         "{} from {}",
//!         info.device_name_value().unwrap_or("Unknown"),
//!         info.ip_value().unwrap_or("?"),
//!     )
//! }
//! ```

mod device;
mod fingerprint;
mod info;

pub use device::{parse_device_name, parse_device_type};
pub use fingerprint::compute_fingerprint;
pub use info::{ClientInfo, header_str};
