//! Session-based authentication and Argon2id password hashing for modo applications.
//!
//! # Overview
//!
//! `modo-auth` provides three building blocks:
//!
//! - **[`UserProvider`]** — implement this trait on your user repository to look up users by ID.
//! - **[`Auth<U>`] / [`OptionalAuth<U>`]** — axum extractors that resolve the current user from
//!   the session and the registered [`UserProviderService<U>`].
//! - **[`PasswordHasher`]** — Argon2id hashing service configured via [`PasswordConfig`].
//!
//! An optional **`templates`** feature adds `UserContextLayer`, a Tower middleware that injects
//! the authenticated user into the minijinja template context under the key `"user"`.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use modo_auth::{UserProvider, UserProviderService, PasswordHasher};
//!
//! struct UserRepo { /* db pool */ }
//!
//! impl UserProvider for UserRepo {
//!     type User = MyUser;
//!
//!     async fn find_by_id(&self, id: &str) -> Result<Option<MyUser>, modo::Error> {
//!         // load from DB
//!         todo!()
//!     }
//! }
//!
//! #[modo::main]
//! async fn main(app: modo::app::AppBuilder, config: Config) -> Result<(), Box<dyn std::error::Error>> {
//!     let repo = UserRepo { /* ... */ };
//!     let hasher = PasswordHasher::default();
//!
//!     app.service(UserProviderService::new(repo))
//!        .service(hasher)
//!        .run()
//!        .await
//! }
//! ```

pub(crate) mod cache;
#[cfg(feature = "templates")]
pub mod context_layer;
pub mod extractor;
pub mod password;
pub mod provider;

pub use extractor::{Auth, OptionalAuth};
pub use password::{PasswordConfig, PasswordHasher};
pub use provider::{UserProvider, UserProviderService};

#[cfg(feature = "templates")]
pub use context_layer::UserContextLayer;
