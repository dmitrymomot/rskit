//! Handler-time prelude.
//!
//! `use modo::prelude::*;` in a handler or middleware module brings in the
//! ambient types reached for on nearly every request:
//!
//! - [`Error`], [`Result`] — the framework error type and alias used by `?`.
//! - [`AppState`] — the shared application state handle.
//! - [`Session`], [`Role`] — session and role extractors. Included because
//!   apps that enable them tend to want them in almost every handler; omit
//!   the glob import in crates that don't use `auth`.
//! - [`Flash`] — per-request flash messages.
//! - [`ClientIp`] — resolved client IP extractor.
//! - [`Tenant`], [`TenantId`] — multi-tenant extractor and identifier.
//! - [`Validate`], [`ValidationError`], [`Validator`] — request-body
//!   validation trait, error, and fluent helper.
//!
//! Less-universal extractors and domain types (JWT claims, OAuth providers,
//! API keys, mailer, template engine, job enqueuer, storage buckets, SSE
//! broadcaster, etc.) are intentionally NOT preluded — import them
//! explicitly from their feature-gated modules where used.

pub use crate::error::{Error, Result};
pub use crate::service::AppState;

pub use crate::auth::role::Role;
pub use crate::auth::session::Session;

pub use crate::flash::Flash;
pub use crate::ip::ClientIp;
pub use crate::tenant::{Tenant, TenantId};
pub use crate::validate::{Validate, ValidationError, Validator};
