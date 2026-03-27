//! Domain-verified signup.
//!
//! Lets tenants claim email domains so that users with matching verified
//! email addresses auto-join the tenant. Domain ownership is proved via
//! DNS TXT record verification using the [`dns`](crate::dns) module.
//!
//! # Feature flag
//!
//! This module is only compiled when the `dns` feature is enabled.
//!
//! ```toml
//! [dependencies]
//! modo = { version = "*", features = ["dns"] }
//! ```

mod registry;
mod types;
mod validate;

pub use registry::DomainRegistry;
pub use types::{ClaimStatus, DomainClaim, TenantMatch};
