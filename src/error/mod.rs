//! # modo::error
//!
//! HTTP-aware error type for the modo web framework.
//!
//! This module provides [`Error`], an opinionated error type that carries an HTTP status code,
//! a human-readable message, an optional structured details payload, an optional source error, and
//! an optional machine-readable error code. `Error` implements [`axum::response::IntoResponse`],
//! so it can be returned directly from axum handlers.
//!
//! Provides:
//! - [`Error`] — primary framework error with status code, message, optional source/details/code
//! - [`Result<T>`] — type alias for `std::result::Result<T, Error>`
//! - [`HttpError`] — lightweight `Copy` enum of common HTTP error statuses, converts into `Error`
//!
//! Automatic `From` conversions are provided for [`std::io::Error`], [`serde_json::Error`],
//! and [`serde_yaml_ng::Error`].
//!
//! # Quick start
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
//! # Error identity
//!
//! After `Error` is converted into an HTTP response, the source error is discarded. Use
//! [`Error::with_code`] to attach a static code string that survives through the response
//! pipeline and can be read back via [`Error::error_code`].
//!
//! ```rust
//! use modo::error::Error;
//!
//! let err = Error::unauthorized("unauthorized")
//!     .with_code("jwt:expired");
//! assert_eq!(err.error_code(), Some("jwt:expired"));
//! ```

mod convert;
mod core;
mod http_error;

pub use core::{Error, Result};
pub use http_error::HttpError;
