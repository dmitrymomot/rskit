pub mod service;

/// Re-export of [`crate::request_id::RequestId`] for ergonomic import via `modo::extractor`.
pub use crate::request_id::RequestId;
/// Re-export of form and JSON request extractors with auto-sanitization.
pub use crate::validate::{FormReq, JsonReq};
/// Path parameter extractor — use in handler signatures as `PathReq<(String,)>`.
pub use axum::extract::Path as PathReq;
/// Query string extractor — use in handler signatures as `QueryReq<MyParams>`.
pub use axum::extract::Query as QueryReq;
/// Service registry extractor — retrieves a registered service by type.
pub use service::Service;
