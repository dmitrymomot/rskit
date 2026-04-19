//! # modo::error
//!
//! HTTP-aware error type for the modo web framework.
//!
//! [`Error`] carries an HTTP status code, a human-readable message, an optional
//! structured details payload, an optional boxed source error, an optional
//! static machine-readable `error_code`, and an optional i18n `locale_key`. It
//! implements [`axum::response::IntoResponse`], so `async fn` handlers can
//! return [`Result<T>`] and use `?` everywhere.
//!
//! ## Provides
//!
//! - [`Error`] — primary framework error type with status + message + optional
//!   source / details / error_code / locale_key
//! - [`Result`] — type alias for `std::result::Result<T, Error>`
//! - [`HttpError`] — lightweight `Copy` enum of common HTTP error statuses; converts
//!   into [`Error`] via `From<HttpError>`
//! - Status-code constructors: [`Error::bad_request`], [`Error::unauthorized`],
//!   [`Error::forbidden`], [`Error::not_found`], [`Error::conflict`],
//!   [`Error::payload_too_large`], [`Error::unprocessable_entity`],
//!   [`Error::too_many_requests`], [`Error::internal`], [`Error::bad_gateway`],
//!   [`Error::gateway_timeout`]
//! - General constructors: [`Error::new`], [`Error::with_source`],
//!   [`Error::localized`], [`Error::lagged`]
//! - Builder methods: [`Error::chain`], [`Error::with_code`],
//!   [`Error::with_details`], [`Error::with_locale_key`]
//! - Automatic [`From`] conversions into [`Error`]:
//!   [`std::io::Error`] → 500, [`serde_json::Error`] → 400,
//!   [`serde_yaml_ng::Error`] → 500
//!
//! ## Quick start
//!
//! ```rust
//! use modo::error::{Error, Result};
//!
//! fn find_user(id: u64) -> Result<String> {
//!     if id == 0 {
//!         return Err(Error::not_found("user not found"));
//!     }
//!     Ok("Alice".to_string())
//! }
//! ```
//!
//! ## Source drops on clone and response — use `error_code` for identity
//!
//! The boxed `source` field is `Box<dyn std::error::Error + Send + Sync>`, which
//! is not `Clone`. Both [`Clone`] and [`Error::into_response`] therefore drop
//! the source. This means:
//!
//! - **Pre-response (inside a handler or middleware that still owns the
//!   `Error`)** — use [`Error::source_as::<T>`](Error::source_as) to downcast
//!   the source to a concrete type.
//! - **Post-response (middleware that reads the error copy stored in response
//!   extensions)** — the source is gone; use [`Error::error_code`] to recover
//!   the identity of the error. Attach it up front with [`Error::with_code`].
//!
//! A typical pattern is `.chain(e).with_code(e.code())` so you keep both the
//! causal source (inspectable pre-response) and the static code (stable
//! post-response).
//!
//! ```rust
//! use modo::error::Error;
//!
//! let err = Error::unauthorized("token expired").with_code("jwt:expired");
//! assert_eq!(err.error_code(), Some("jwt:expired"));
//! ```
//!
//! ## Usage in middleware and guards
//!
//! Middleware and route guards that need to short-circuit with an error should
//! build an [`Error`] and call [`IntoResponse::into_response`](axum::response::IntoResponse::into_response)
//! — never construct raw [`axum::response::Response`] values by hand. This
//! ensures the JSON body shape and response-extension copy stay consistent
//! across the framework.

mod convert;
mod core;
mod http_error;

pub(crate) use core::render_error_body;
pub use core::{Error, Result};
pub use http_error::HttpError;
