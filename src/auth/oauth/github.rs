use axum_extra::extract::cookie::Key;

use crate::cookie::CookieConfig;

use super::{
    client,
    config::{CallbackParams, OAuthProviderConfig},
    profile::UserProfile,
    provider::OAuthProvider,
    state::{AuthorizationRequest, OAuthState, build_oauth_cookie, pkce_challenge},
};

const AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const USER_URL: &str = "https://api.github.com/user";
const EMAILS_URL: &str = "https://api.github.com/user/emails";
const DEFAULT_SCOPES: &[&str] = &["user:email", "read:user"];

/// OAuth 2.0 provider implementation for GitHub.
///
/// Implements the Authorization Code flow with PKCE (S256). Default scopes are
/// `user:email` and `read:user`. Override them via
/// [`OAuthProviderConfig::scopes`](super::OAuthProviderConfig::scopes).
///
/// The primary verified email is fetched from the GitHub `/user/emails` endpoint and used to
/// populate [`UserProfile::email`](super::UserProfile::email) and
/// [`UserProfile::email_verified`](super::UserProfile::email_verified).
///
/// Requires the `auth` feature.
pub struct GitHub {
    config: OAuthProviderConfig,
    cookie_config: CookieConfig,
    key: Key,
    http_client: reqwest::Client,
}

impl GitHub {
    /// Creates a new `GitHub` provider from the given configuration.
    ///
    /// `cookie_config` and `key` are used to sign the `_oauth_state` cookie that carries the
    /// PKCE verifier and state nonce across the redirect. `http_client` is a
    /// [`reqwest::Client`] used for the token exchange and user-info API calls.
    pub fn new(
        config: &OAuthProviderConfig,
        cookie_config: &CookieConfig,
        key: &Key,
        http_client: reqwest::Client,
    ) -> Self {
        Self {
            config: config.clone(),
            cookie_config: cookie_config.clone(),
            key: key.clone(),
            http_client,
        }
    }

    fn scopes(&self) -> String {
        if self.config.scopes.is_empty() {
            DEFAULT_SCOPES.join(" ")
        } else {
            self.config.scopes.join(" ")
        }
    }
}

impl OAuthProvider for GitHub {
    fn name(&self) -> &str {
        "github"
    }

    fn authorize_url(&self) -> crate::Result<AuthorizationRequest> {
        let (set_cookie_header, state_nonce, pkce_verifier) =
            build_oauth_cookie("github", &self.key, &self.cookie_config);

        let challenge = pkce_challenge(&pkce_verifier);

        let redirect_url = format!(
            "{AUTHORIZE_URL}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(&self.scopes()),
            urlencoding::encode(&state_nonce),
            urlencoding::encode(&challenge),
        );

        Ok(AuthorizationRequest {
            redirect_url,
            set_cookie_header,
        })
    }

    async fn exchange(
        &self,
        params: &CallbackParams,
        state: &OAuthState,
    ) -> crate::Result<UserProfile> {
        if state.provider() != "github" {
            return Err(crate::Error::bad_request("OAuth state provider mismatch"));
        }

        if params.state != state.state_nonce() {
            return Err(crate::Error::bad_request("OAuth state nonce mismatch"));
        }

        #[derive(serde::Deserialize)]
        struct TokenResponse {
            access_token: String,
        }

        let token: TokenResponse = client::post_form(
            &self.http_client,
            TOKEN_URL,
            &[
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("code", &params.code),
                ("redirect_uri", &self.config.redirect_uri),
                ("code_verifier", state.pkce_verifier()),
            ],
        )
        .await?;

        let raw: serde_json::Value =
            client::get_json(&self.http_client, USER_URL, &token.access_token).await?;

        let provider_user_id = raw["id"]
            .as_u64()
            .map(|id| id.to_string())
            .or_else(|| raw["id"].as_str().map(|s| s.to_string()))
            .ok_or_else(|| crate::Error::internal("github: missing user id"))?;

        let name = raw["name"].as_str().map(|s| s.to_string());
        let avatar_url = raw["avatar_url"].as_str().map(|s| s.to_string());

        #[derive(serde::Deserialize)]
        struct GitHubEmail {
            email: String,
            primary: bool,
            verified: bool,
        }

        let emails: Vec<GitHubEmail> =
            client::get_json(&self.http_client, EMAILS_URL, &token.access_token).await?;

        let primary = emails
            .iter()
            .find(|e| e.primary)
            .ok_or_else(|| crate::Error::internal("github: no primary email"))?;

        Ok(UserProfile {
            provider: "github".to_string(),
            provider_user_id,
            email: primary.email.clone(),
            email_verified: primary.verified,
            name,
            avatar_url,
            raw,
        })
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(b as char);
                }
                _ => {
                    result.push_str(&format!("%{b:02X}"));
                }
            }
        }
        result
    }
}
