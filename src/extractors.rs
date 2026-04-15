//! Flat index of every axum extractor modo ships.
//!
//! Re-exports extractor types from across the crate (request bodies, auth,
//! flash, client IP, tenant, SSE, templates, tier) so you can reach for any
//! of them from a single path: `use modo::extractors::*;` or
//! `modo::extractors::JsonRequest`.
//!
//! For the handful of extractors used in nearly every handler (`Session`,
//! `Role`, `Flash`, `ClientIp`, `Tenant`, `AppState`), prefer
//! [`modo::prelude`](crate::prelude) — it also brings in `Error`/`Result`
//! and the validation traits. This module is the exhaustive index; the
//! prelude is the ergonomic default.

pub use crate::extractor::{FormRequest, JsonRequest, MultipartRequest, Query, UploadedFile};

pub use crate::auth::apikey::ApiKeyMeta;
pub use crate::auth::role::Role;
pub use crate::auth::session::Session;
pub use crate::auth::session::jwt::{Bearer, Claims};

pub use crate::flash::Flash;
pub use crate::ip::{ClientInfo, ClientIp};
pub use crate::service::AppState;
pub use crate::sse::LastEventId;
pub use crate::template::HxRequest;
pub use crate::tenant::Tenant;
pub use crate::tier::TierInfo;
