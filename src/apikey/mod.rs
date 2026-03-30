//! Prefixed API key issuance, verification, scoping, and lifecycle management.
//!
//! Provides tenant-scoped API keys with SHA-256 hashing, constant-time
//! verification, touch throttling, and Tower middleware for request
//! authentication.
//!
//! # Feature flag
//!
//! This module is only compiled when the `apikey` feature is enabled.
//!
//! ```toml
//! [dependencies]
//! modo = { version = "*", features = ["apikey"] }
//! ```

mod backend;
mod config;
mod extractor;
mod middleware;
mod scope;
pub(crate) mod sqlite;
mod store;
mod token;
mod types;

pub use backend::ApiKeyBackend;
pub use config::ApiKeyConfig;
pub use middleware::ApiKeyLayer;
pub use scope::require_scope;
pub use store::ApiKeyStore;
pub use types::{ApiKeyCreated, ApiKeyMeta, ApiKeyRecord, CreateKeyRequest};
