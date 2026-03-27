//! Request extractors for the modo web framework.
//!
//! All extractors in this module integrate with [`crate::sanitize::Sanitize`]:
//! [`JsonRequest`], [`FormRequest`], [`Query`], and [`MultipartRequest`] all
//! call [`crate::sanitize::Sanitize::sanitize`] on the deserialized value before
//! returning it, so whitespace trimming and other normalization happen
//! automatically.
//!
//! [`Path`] is re-exported directly from axum and behaves identically.
//!
//! # Extractors overview
//!
//! | Extractor | Source | Trait bound |
//! |---|---|---|
//! | [`JsonRequest<T>`] | JSON body | `T: DeserializeOwned + Sanitize` |
//! | [`FormRequest<T>`] | URL-encoded form body | `T: DeserializeOwned + Sanitize` |
//! | [`Query<T>`] | URL query string | `T: DeserializeOwned + Sanitize` |
//! | [`MultipartRequest<T>`] | `multipart/form-data` body | `T: DeserializeOwned + Sanitize` |
//! | [`Path`] | URL path parameters | `T: DeserializeOwned` |
//! | [`Service<T>`] | Service registry | `T: Send + Sync + 'static` |

mod form;
mod json;
mod multipart;
mod query;
mod service;
mod upload_validator;

pub use axum::extract::Path;
pub use form::FormRequest;
pub use json::JsonRequest;
pub use multipart::{Files, MultipartRequest, UploadedFile};
pub use query::Query;
pub use service::Service;
pub use upload_validator::UploadValidator;
