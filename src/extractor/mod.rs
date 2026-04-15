//! # modo::extractor
//!
//! Request extractors for the modo web framework.
//!
//! All sanitizing extractors call [`crate::sanitize::Sanitize::sanitize`] on
//! the deserialized value before returning it, so whitespace trimming and other
//! normalization happen automatically.
//!
//! [`Path`] is re-exported directly from axum and behaves identically.
//!
//! ## Extractors
//!
//! | Extractor | Source | Trait bound |
//! |---|---|---|
//! | [`JsonRequest<T>`] | JSON body | `T: DeserializeOwned + Sanitize` |
//! | [`FormRequest<T>`] | URL-encoded form body | `T: DeserializeOwned + Sanitize` |
//! | [`Query<T>`] | URL query string | `T: DeserializeOwned + Sanitize` |
//! | [`MultipartRequest<T>`] | `multipart/form-data` body | `T: DeserializeOwned + Sanitize` |
//! | [`Path`] | URL path parameters | `T: DeserializeOwned` |
//!
//! ## Multipart helpers
//!
//! | Type | Purpose |
//! |---|---|
//! | [`UploadedFile`] | Single file extracted from a multipart field; also constructable via [`UploadedFile::from_field`] for advanced use |
//! | [`Files`] | Map of field names to uploaded files; constructable via [`Files::from_map`] for testing or pre-built maps |
//! | [`UploadValidator`] | Fluent size/content-type validator for [`UploadedFile`] |

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
