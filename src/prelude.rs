//! Common imports for handlers and middleware.
//!
//! `use modo::prelude::*;` brings in the ambient types reached for in
//! almost every request handler — including the `Session` and `Role`
//! extractors, since handlers in apps that use them tend to want them
//! everywhere. Less-universal extractors and domain types (JWT claims,
//! OAuth providers, mailer, template engine, etc.) are NOT preluded —
//! import them explicitly where used.

pub use crate::error::{Error, Result};
pub use crate::service::AppState;

pub use crate::auth::role::Role;
pub use crate::auth::session::Session;

pub use crate::flash::Flash;
pub use crate::ip::ClientIp;
pub use crate::tenant::{Tenant, TenantId};
pub use crate::validate::{Validate, ValidationError, Validator};
