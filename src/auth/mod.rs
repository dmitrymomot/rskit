//! # modo::auth
//!
//! Identity and access — sessions, JWT, OAuth, API keys, roles, and route-level guards.
//!
//! Umbrella module for authenticating callers and gating routes. Each
//! capability lives in its own submodule; the [`guard`] file houses the
//! route-level layers (`require_authenticated`, `require_unauthenticated`,
//! `require_role`, `require_scope`) that compose with the rest.
//!
//! ## Provides
//!
//! Submodules:
//!
//! - [`apikey`] — prefixed API key issuance, verification, and lifecycle
//!   (store, backend, middleware, scope metadata).
//! - [`oauth`] — OAuth 2.0 Authorization Code + PKCE integrations (Google,
//!   GitHub) plus the [`oauth::OAuthProvider`] trait for custom providers.
//! - [`role`] — role resolution: [`role::RoleExtractor`] trait, role
//!   [`role::middleware`], and the [`role::Role`] request extension.
//! - [`session`] — database-backed HTTP session management for both cookie
//!   and JWT transports, sharing one [`session::Session`] data type.
//! - [`guard`] — route-level gating layers ([`guard::require_authenticated`],
//!   [`guard::require_unauthenticated`], [`guard::require_role`],
//!   [`guard::require_scope`]).
//! - [`password`] — Argon2id password hashing and verification.
//! - [`otp`] — numeric one-time password generation and verification.
//! - [`totp`] — RFC 6238 TOTP authenticator (Google Authenticator compatible).
//! - [`backup`] — one-time backup recovery codes.
//! - [`jwt`] — back-compat alias for [`session::jwt`] so `modo::auth::jwt::*`
//!   keeps working.
//!
//! Convenience re-exports at `modo::auth`:
//!
//! - [`PasswordConfig`] — Argon2id hashing parameters.
//! - [`Totp`], [`TotpConfig`] — TOTP authenticator and parameters.
//! - [`Claims`] — standard JWT registered claims; axum extractor.
//! - [`JwtSessionsConfig`], [`JwtConfig`] — JWT YAML config (alias preserved).
//! - [`JwtEncoder`], [`JwtDecoder`] — low-level JWT encode/decode.
//! - [`JwtLayer`] — Tower middleware that enforces JWT auth.
//! - [`JwtError`] — typed JWT error enum with static `code()` strings.
//! - [`Bearer`] — extractor for raw Bearer token strings.
//! - [`HmacSigner`], [`TokenSigner`], [`TokenVerifier`] — HS256 signer and traits.
//! - [`TokenSource`], [`TokenSourceConfig`] — pluggable token extraction.
//! - [`ValidationConfig`] — JWT validation policy (leeway, issuer, audience).
//!
//! ## Example
//!
//! Gate an admin route with [`guard::require_role`] and require any
//! authenticated session on the app tree with [`guard::require_authenticated`]:
//!
//! ```rust,no_run
//! use modo::axum::{Router, routing::get};
//! use modo::auth::guard;
//!
//! let app: Router = Router::new()
//!     .route("/me", get(|| async { "profile" }))
//!     .route_layer(guard::require_authenticated("/auth"))
//!     .route("/admin", get(|| async { "admin" }))
//!     .route_layer(guard::require_role(["admin"]));
//! ```

pub mod apikey;
pub mod backup;
pub mod guard;
pub mod otp;
pub mod password;
pub mod role;
pub mod session;
pub mod totp;

pub mod oauth;

mod internal;

// Back-compat re-export — jwt now lives at `auth::session::jwt`.
// This alias keeps `modo::auth::jwt::*` working without breakage.
pub use crate::auth::session::jwt;

// Convenience re-exports
pub use password::PasswordConfig;
pub use totp::{Totp, TotpConfig};

pub use jwt::{
    Bearer, Claims, HmacSigner, JwtConfig, JwtDecoder, JwtEncoder, JwtError, JwtLayer,
    JwtSessionsConfig, TokenSigner, TokenSource, TokenSourceConfig, TokenVerifier,
    ValidationConfig,
};
