use axum_extra::extract::cookie::Key;

use crate::cookie::CookieConfig;

use super::{
    client,
    config::{CallbackParams, OAuthProviderConfig},
    profile::UserProfile,
    provider::OAuthProvider,
    state::{AuthorizationRequest, OAuthState, build_oauth_cookie, pkce_challenge},
};

const AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const DEFAULT_SCOPES: &[&str] = &["openid", "email", "profile"];

/// OAuth 2.0 provider implementation for Google.
///
/// Implements the Authorization Code flow with PKCE (S256). Default scopes are
/// `openid`, `email`, and `profile`. Override them via
/// [`OAuthProviderConfig::scopes`](super::OAuthProviderConfig::scopes).
///
/// Requires the `auth` feature.
pub struct Google {
    config: OAuthProviderConfig,
    cookie_config: CookieConfig,
    key: Key,
    http_client: reqwest::Client,
}

impl Google {
    /// Creates a new `Google` provider from the given configuration.
    ///
    /// `cookie_config` and `key` are used to sign the `_oauth_state` cookie that carries the
    /// PKCE verifier and state nonce across the redirect.
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

impl OAuthProvider for Google {
    fn name(&self) -> &str {
        "google"
    }

    fn authorize_url(&self) -> crate::Result<AuthorizationRequest> {
        let (set_cookie_header, state_nonce, pkce_verifier) =
            build_oauth_cookie("google", &self.key, &self.cookie_config);

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
        if state.provider() != "google" {
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
                ("grant_type", "authorization_code"),
                ("code", &params.code),
                ("redirect_uri", &self.config.redirect_uri),
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("code_verifier", state.pkce_verifier()),
            ],
        )
        .await?;

        let raw: serde_json::Value =
            client::get_json(&self.http_client, USERINFO_URL, &token.access_token).await?;

        let provider_user_id = raw["id"]
            .as_str()
            .ok_or_else(|| crate::Error::internal("google: missing user id"))?
            .to_string();
        let email = raw["email"]
            .as_str()
            .ok_or_else(|| crate::Error::internal("google: missing email"))?
            .to_string();
        let email_verified = raw["verified_email"].as_bool().unwrap_or(false);
        let name = raw["name"].as_str().map(|s| s.to_string());
        let avatar_url = raw["picture"].as_str().map(|s| s.to_string());

        Ok(UserProfile {
            provider: "google".to_string(),
            provider_user_id,
            email,
            email_verified,
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
