//! OAuth 2.0 Authorization Code flow with PKCE for modo applications.
//!
//! Provides built-in provider implementations for Google and GitHub, a shared
//! [`OAuthProvider`] trait for custom providers, and the axum types needed to wire
//! the login and callback routes.
//!
//! Requires the `auth` feature flag.
//!
//! # Flow overview
//!
//! 1. **Login route** — call [`OAuthProvider::authorize_url`] and return the
//!    [`AuthorizationRequest`] directly. It issues a `303 See Other` redirect to the
//!    provider and sets a signed `_oauth_state` cookie (5-minute TTL) that stores the
//!    PKCE verifier and state nonce.
//! 2. **Callback route** — extract [`OAuthState`] (verifies and reads the cookie) and
//!    [`CallbackParams`] (the `?code=…&state=…` query params) from the request, then call
//!    [`OAuthProvider::exchange`] to obtain a [`UserProfile`].

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
