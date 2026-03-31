//! # modo::auth::oauth
//!
//! OAuth 2.0 Authorization Code flow with PKCE for modo applications.
//!
//! Provides built-in provider implementations for Google and GitHub, a shared
//! [`OAuthProvider`] trait for custom providers, and the axum types needed to wire
//! the login and callback routes.
//!
//! Requires feature `"auth"`.
//!
//! ## Provides
//!
//! | Export | Kind | Description |
//! |--------|------|-------------|
//! | [`OAuthProvider`] | trait | Abstraction for implementing custom OAuth 2.0 providers (not object-safe) |
//! | [`Google`] | struct | Built-in Google provider (default scopes: `openid`, `email`, `profile`) |
//! | [`GitHub`] | struct | Built-in GitHub provider (default scopes: `user:email`, `read:user`) |
//! | [`OAuthConfig`] | struct | Top-level YAML configuration for all providers |
//! | [`OAuthProviderConfig`] | struct | Per-provider credentials (`client_id`, `client_secret`, `redirect_uri`, `scopes`) |
//! | [`AuthorizationRequest`] | struct | `IntoResponse` redirect that sets the `_oauth_state` cookie |
//! | [`OAuthState`] | struct | axum extractor that reads and verifies the `_oauth_state` cookie |
//! | [`CallbackParams`] | struct | Deserialized `?code=...&state=...` query params from the provider callback |
//! | [`UserProfile`] | struct | Normalized user profile returned after a successful exchange |
//!
//! ## Flow overview
//!
//! 1. **Login route** -- call [`OAuthProvider::authorize_url`] and return the
//!    [`AuthorizationRequest`] directly. It issues a `303 See Other` redirect to the
//!    provider and sets a signed `_oauth_state` cookie (5-minute TTL) that stores the
//!    PKCE verifier and state nonce.
//! 2. **Callback route** -- extract [`OAuthState`] (verifies and reads the cookie) and
//!    [`CallbackParams`] (the `?code=...&state=...` query params) from the request, then
//!    call [`OAuthProvider::exchange`] to obtain a [`UserProfile`].

mod client;
mod config;
mod github;
mod google;
mod profile;
mod provider;
mod state;

pub use config::{CallbackParams, OAuthConfig, OAuthProviderConfig};
pub use github::GitHub;
pub use google::Google;
pub use profile::UserProfile;
pub use provider::OAuthProvider;
pub use state::{AuthorizationRequest, OAuthState};
