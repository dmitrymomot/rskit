//! # modo::sanitize
//!
//! Input sanitization utilities for normalizing string fields before validation
//! or storage.
//!
//! Provides:
//!
//! | Item | Kind | Purpose |
//! |------|------|---------|
//! | [`Sanitize`] | trait | Implemented on input structs to normalize fields in place |
//! | [`trim`] | fn | Trim leading and trailing whitespace |
//! | [`trim_lowercase`] | fn | Trim whitespace and convert to lowercase |
//! | [`collapse_whitespace`] | fn | Collapse consecutive whitespace into a single space |
//! | [`strip_html`] | fn | Remove HTML tags and decode entities |
//! | [`truncate`] | fn | Limit string to a maximum character count |
//! | [`normalize_email`] | fn | Trim, lowercase, and strip `+tag` suffixes |
//!
//! The [`JsonRequest`](crate::extractor::JsonRequest),
//! [`FormRequest`](crate::extractor::FormRequest),
//! [`Query`](crate::extractor::Query), and
//! [`MultipartRequest`](crate::extractor::MultipartRequest) extractors call
//! [`Sanitize::sanitize`] automatically after deserialization, so implementing
//! the trait on a request struct is the only wiring required.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use modo::sanitize::{Sanitize, trim, normalize_email};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct SignupInput {
//!     username: String,
//!     email: String,
//! }
//!
//! impl Sanitize for SignupInput {
//!     fn sanitize(&mut self) {
//!         trim(&mut self.username);
//!         normalize_email(&mut self.email);
//!     }
//! }
//! ```

mod functions;
mod html;
mod traits;

pub use functions::{
    collapse_whitespace, normalize_email, strip_html, trim, trim_lowercase, truncate,
};
pub use traits::Sanitize;
