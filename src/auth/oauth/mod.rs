//! # modo::auth::oauth
//!
//! OAuth 2.0 Authorization Code flow with PKCE (S256) for modo applications.
//!
//! Provides built-in provider implementations for Google and GitHub, a shared
//! [`OAuthProvider`] trait for custom providers, and the axum types needed to
//! wire login and callback routes.
//!
//! Provides:
//! - [`OAuthProvider`] — trait abstraction for OAuth 2.0 providers (not object-safe — uses RPITIT)
//! - [`Google`] — built-in Google provider (default scopes: `openid`, `email`, `profile`)
//! - [`GitHub`] — built-in GitHub provider (default scopes: `user:email`, `read:user`)
//! - [`OAuthConfig`] — top-level YAML configuration for all providers
//! - [`OAuthProviderConfig`] — per-provider credentials (`client_id`, `client_secret`, `redirect_uri`, `scopes`)
//! - [`AuthorizationRequest`] — `IntoResponse` redirect that sets the `_oauth_state` cookie
//! - [`OAuthState`] — axum extractor that reads and verifies the `_oauth_state` cookie
//! - [`CallbackParams`] — deserialized `?code=...&state=...` query params from the provider callback
//! - [`UserProfile`] — normalized user profile returned after a successful exchange
//!
//! ## Flow overview
//!
//! 1. **Login route** — call [`OAuthProvider::authorize_url`] and return the
//!    [`AuthorizationRequest`] directly. It issues a `303 See Other` redirect to the
//!    provider and sets a signed `_oauth_state` cookie (5-minute TTL) that stores the
//!    PKCE verifier and state nonce.
//! 2. **Callback route** — extract [`OAuthState`] (verifies and reads the cookie) and
//!    [`CallbackParams`] (the `?code=...&state=...` query params) from the request, then
//!    call [`OAuthProvider::exchange`] to obtain a [`UserProfile`].
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use axum::{Router, response::{IntoResponse, Redirect, Response}, routing::get, extract::Query};
//! use modo::auth::oauth::{CallbackParams, Google, OAuthConfig, OAuthProvider, OAuthState, UserProfile};
//! use modo::cookie::{CookieConfig, key_from_config};
//! use modo::service::{Registry, Service};
//!
//! async fn login(Service(google): Service<Google>) -> modo::Result<Response> {
//!     Ok(google.authorize_url()?.into_response())
//! }
//!
//! async fn callback(
//!     oauth_state: OAuthState,
//!     Query(params): Query<CallbackParams>,
//!     Service(google): Service<Google>,
//! ) -> modo::Result<Redirect> {
//!     let _profile: UserProfile = google.exchange(&params, &oauth_state).await?;
//!     Ok(Redirect::to("/dashboard"))
//! }
//!
//! fn build(oauth: &OAuthConfig, cookie: &CookieConfig, http: reqwest::Client) -> Router {
//!     let key = key_from_config(cookie).expect("cookie secret must be at least 64 chars");
//!     let mut registry = Registry::new();
//!     registry.add(key.clone());
//!     if let Some(cfg) = &oauth.google {
//!         registry.add(Google::new(cfg, cookie, &key, http.clone()));
//!     }
//!     Router::new()
//!         .route("/auth/google", get(login))
//!         .route("/auth/google/callback", get(callback))
//!         .with_state(registry.into_state())
//! }
//! ```

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
