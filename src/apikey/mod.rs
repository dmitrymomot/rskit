//! # modo::apikey
//!
//! Prefixed API key issuance, verification, scoping, and lifecycle management.
//!
//! Requires feature `"apikey"` (depends on `"db"`).
//!
//! ```toml
//! [dependencies]
//! modo = { version = "0.5", features = ["apikey"] }
//! ```
//!
//! ## Provides
//!
//! ### Core
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`ApiKeyStore`] | Tenant-scoped store: create, verify, revoke, list, refresh keys |
//! | [`ApiKeyConfig`] | YAML-deserializable configuration (prefix, secret length, touch threshold) |
//! | [`ApiKeyBackend`] | Trait for pluggable storage backends (SQLite built-in) |
//!
//! ### Middleware
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`ApiKeyLayer`] | Tower layer that verifies API keys on incoming requests |
//! | [`require_scope`] | Tower layer factory that enforces a required scope on verified keys |
//!
//! ### Data types
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`ApiKeyMeta`] | Public metadata extracted by middleware, usable as an axum extractor |
//! | [`ApiKeyCreated`] | One-time creation result containing the raw token |
//! | [`ApiKeyRecord`] | Full stored record used by backend implementations |
//! | [`CreateKeyRequest`] | Input for [`ApiKeyStore::create`] |
//!
//! ### Testing
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`test::InMemoryBackend`] | In-memory backend for unit tests (requires `test-helpers` feature) |
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use modo::apikey::{ApiKeyConfig, ApiKeyStore, ApiKeyLayer, CreateKeyRequest, require_scope};
//! use axum::{Router, routing::get};
//! # fn example(db: modo::db::Database) {
//!
//! // Build the store from config + database
//! let store = ApiKeyStore::new(db, ApiKeyConfig::default()).unwrap();
//!
//! // Protect routes with the API key middleware and optional scope checks
//! let app: Router = Router::new()
//!     .route("/orders", get(|| async { "orders" }))
//!     .route_layer(require_scope("read:orders"))
//!     .layer(ApiKeyLayer::new(store));
//! # }
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

/// Test helpers for the API key module.
///
/// Available when running tests or when the `test-helpers` feature is enabled.
#[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]
pub mod test {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    use crate::error::Result;

    use super::backend::ApiKeyBackend;
    use super::types::ApiKeyRecord;

    /// In-memory backend for unit tests.
    pub struct InMemoryBackend {
        records: Mutex<Vec<ApiKeyRecord>>,
    }

    impl Default for InMemoryBackend {
        fn default() -> Self {
            Self::new()
        }
    }

    impl InMemoryBackend {
        /// Create an empty in-memory backend.
        pub fn new() -> Self {
            Self {
                records: Mutex::new(Vec::new()),
            }
        }
    }

    impl ApiKeyBackend for InMemoryBackend {
        fn store(
            &self,
            record: &ApiKeyRecord,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
            self.records.lock().unwrap().push(record.clone());
            Box::pin(async { Ok(()) })
        }

        fn lookup(
            &self,
            key_id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Option<ApiKeyRecord>>> + Send + '_>> {
            let found = self
                .records
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.id == key_id)
                .cloned();
            Box::pin(async { Ok(found) })
        }

        fn revoke(
            &self,
            key_id: &str,
            revoked_at: &str,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
            let revoked_at = revoked_at.to_owned();
            if let Some(r) = self
                .records
                .lock()
                .unwrap()
                .iter_mut()
                .find(|r| r.id == key_id)
            {
                r.revoked_at = Some(revoked_at);
            }
            Box::pin(async { Ok(()) })
        }

        fn list(
            &self,
            tenant_id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<ApiKeyRecord>>> + Send + '_>> {
            let records: Vec<ApiKeyRecord> = self
                .records
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.tenant_id == tenant_id && r.revoked_at.is_none())
                .cloned()
                .collect();
            Box::pin(async { Ok(records) })
        }

        fn update_last_used(
            &self,
            key_id: &str,
            timestamp: &str,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
            let timestamp = timestamp.to_owned();
            if let Some(r) = self
                .records
                .lock()
                .unwrap()
                .iter_mut()
                .find(|r| r.id == key_id)
            {
                r.last_used_at = Some(timestamp);
            }
            Box::pin(async { Ok(()) })
        }

        fn update_expires_at(
            &self,
            key_id: &str,
            expires_at: Option<&str>,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
            let expires_at = expires_at.map(|s| s.to_owned());
            if let Some(r) = self
                .records
                .lock()
                .unwrap()
                .iter_mut()
                .find(|r| r.id == key_id)
            {
                r.expires_at = expires_at;
            }
            Box::pin(async { Ok(()) })
        }
    }
}
