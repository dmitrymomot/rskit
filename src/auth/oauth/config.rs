use serde::Deserialize;

/// Top-level OAuth configuration, typically loaded from the application YAML config.
///
/// Each field corresponds to a provider. Set the field to `None` to disable that provider.
///
/// # Example (YAML)
///
/// ```yaml
/// oauth:
///   google:
///     client_id: "${GOOGLE_CLIENT_ID}"
///     client_secret: "${GOOGLE_CLIENT_SECRET}"
///     redirect_uri: "https://example.com/auth/google/callback"
///   github:
///     client_id: "${GITHUB_CLIENT_ID}"
///     client_secret: "${GITHUB_CLIENT_SECRET}"
///     redirect_uri: "https://example.com/auth/github/callback"
/// ```
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct OAuthConfig {
    pub google: Option<OAuthProviderConfig>,
    pub github: Option<OAuthProviderConfig>,
}

/// Per-provider OAuth 2.0 credentials and settings.
#[derive(Debug, Clone, Deserialize)]
pub struct OAuthProviderConfig {
    /// OAuth application client ID.
    pub client_id: String,
    /// OAuth application client secret.
    pub client_secret: String,
    /// Redirect URI registered with the OAuth provider.
    pub redirect_uri: String,
    /// Optional list of scopes to request. Falls back to sensible provider defaults when empty.
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// Query parameters delivered by the OAuth provider to the callback route.
///
/// Deserialize this from the request query string with axum's `Query<CallbackParams>`.
#[derive(Debug, Clone, Deserialize)]
pub struct CallbackParams {
    /// Authorization code returned by the provider.
    pub code: String,
    /// Opaque state value — must match the nonce stored in the OAuth cookie.
    pub state: String,
}
