use super::{
    config::CallbackParams,
    profile::UserProfile,
    state::{AuthorizationRequest, OAuthState},
};

/// Abstraction over an OAuth 2.0 provider.
///
/// Implement this trait to add a custom provider. The two built-in implementations are
/// [`Google`](super::Google) and [`GitHub`](super::GitHub).
///
/// # Object safety
///
/// `OAuthProvider` is **not** object-safe because `exchange` returns an `impl Future` (RPITIT).
/// Use concrete types or monomorphised generics — do not box this trait.
///
/// # Required methods
///
/// - [`name`](OAuthProvider::name) — returns a stable, lowercase identifier (`"google"`, `"github"`, …).
/// - [`authorize_url`](OAuthProvider::authorize_url) — builds the authorization redirect and
///   issues the PKCE + state cookie.
/// - [`exchange`](OAuthProvider::exchange) — verifies the callback, exchanges the code for a
///   token, and fetches the user profile.
pub trait OAuthProvider: Send + Sync {
    /// A stable, lowercase identifier for this provider (e.g. `"google"`, `"github"`).
    fn name(&self) -> &str;

    /// Builds an authorization redirect response.
    ///
    /// Generates a PKCE verifier, a state nonce, and a signed cookie that binds them to this
    /// provider. Returns an [`AuthorizationRequest`] that implements [`axum::response::IntoResponse`]
    /// — return it directly from an axum handler to redirect the user.
    ///
    /// # Errors
    ///
    /// Returns an error if the authorization URL cannot be constructed.
    fn authorize_url(&self) -> crate::Result<AuthorizationRequest>;

    /// Exchanges an authorization code for a [`UserProfile`].
    ///
    /// Validates that `params.state` matches the nonce stored in `state` and that
    /// `state.provider` matches this provider's [`name`](OAuthProvider::name). Performs the token
    /// exchange and fetches the user's profile from the provider API.
    ///
    /// # Errors
    ///
    /// Returns `Error::bad_request` if the state nonce or provider does not match.
    /// Returns `Error::internal` if the token exchange or profile fetch fails.
    fn exchange(
        &self,
        params: &CallbackParams,
        state: &OAuthState,
    ) -> impl Future<Output = crate::Result<UserProfile>> + Send;
}
