//! Input sanitization utilities for the modo web framework.
//!
//! This module provides the [`Sanitize`] trait and a set of standalone functions
//! for normalizing string fields before validation or storage.  Implementing
//! `Sanitize` on a request struct is the integration point used by the
//! `JsonRequest`, `FormRequest`, `Query`, and `MultipartRequest` extractors —
//! each extractor calls [`Sanitize::sanitize`] automatically after deserialization.
//!
//! # Functions
//!
//! | Function | Operation |
//! |---|---|
//! | [`trim`] | Trim leading and trailing whitespace |
//! | [`trim_lowercase`] | Trim whitespace and convert to lowercase |
//! | [`collapse_whitespace`] | Collapse consecutive whitespace into a single space |
//! | [`strip_html`] | Remove HTML tags and decode entities |
//! | [`truncate`] | Limit string to a maximum character count |
//! | [`normalize_email`] | Trim, lowercase, and strip `+tag` suffixes |
//!
//! # Example
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
