//! # modo::extractor
//!
//! Sanitizing axum extractors for request bodies, query strings, and multipart uploads.
//!
//! Every sanitizing extractor calls [`crate::sanitize::Sanitize::sanitize`] on the
//! deserialized value before returning it, so whitespace trimming and other
//! normalization happen automatically. Rejections are [`crate::Error`] values with
//! `400 Bad Request` status, which render through [`crate::Error::into_response`].
//!
//! `FormRequest`, `Query`, and the text-field side of `MultipartRequest` deserialize
//! through `serde_qs`, so HTML forms can map directly into Rust structs:
//!
//! - Repeated keys (`tag=a&tag=b`) populate `Vec<scalar>` fields — multi-select
//!   checkbox groups and dropdown lists.
//! - Bracketed keys (`address[city]=…`) populate nested struct fields.
//! - Indexed brackets (`contacts[0][kind]=…&contacts[0][value]=…`) populate
//!   `Vec<Struct>` rows — htmx-added dynamic-row groups and per-row contact lists.
//!
//! `FormRequest` and `MultipartRequest` use `serde_qs` form-encoding mode so they
//! accept browser-encoded brackets (`%5B`/`%5D`); `Query<T>` uses default mode so
//! URL templates can keep bare brackets.
//!
//! Provides:
//!
//! - [`JsonRequest<T>`] — JSON body (`T: DeserializeOwned + Sanitize`)
//! - [`FormRequest<T>`] — URL-encoded form body (`T: DeserializeOwned + Sanitize`)
//! - [`Query<T>`] — URL query string (`T: DeserializeOwned + Sanitize`)
//! - [`MultipartRequest<T>`] — `multipart/form-data` body split into text fields
//!   and a [`Files`] map (`T: DeserializeOwned + Sanitize`)
//! - [`Path`] — URL path parameters, re-exported from axum unchanged
//!   (`T: DeserializeOwned`, no sanitization)
//! - [`UploadedFile`] — single file from a multipart field (also directly constructable
//!   via [`UploadedFile::from_field`])
//! - [`Files`] — map of field names to uploaded files (also constructable via
//!   [`Files::from_map`] for tests)
//! - [`UploadValidator`] — fluent size/content-type validator for [`UploadedFile`]

mod form;
mod json;
mod multipart;
mod query;
mod upload_validator;

pub use axum::extract::Path;
pub use form::FormRequest;
pub use json::JsonRequest;
pub use multipart::{Files, MultipartRequest, UploadedFile};
pub use query::Query;
pub use upload_validator::UploadValidator;
