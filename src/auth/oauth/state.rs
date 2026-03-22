use axum::extract::{FromRef, FromRequestParts};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::Key;
use cookie::{Cookie, CookieJar, SameSite};
use http::header::{COOKIE, SET_COOKIE};
use http::request::Parts;

use crate::cookie::CookieConfig;
use crate::service::AppState;

const OAUTH_COOKIE_NAME: &str = "_oauth_state";
const OAUTH_COOKIE_MAX_AGE_SECS: i64 = 300;

pub struct OAuthState {
    state_nonce: String,
    pkce_verifier: String,
    provider: String,
}

impl OAuthState {
    pub(crate) fn provider(&self) -> &str {
        &self.provider
    }

    pub(crate) fn pkce_verifier(&self) -> &str {
        &self.pkce_verifier
    }

    pub(crate) fn state_nonce(&self) -> &str {
        &self.state_nonce
    }

    pub(crate) fn from_signed_cookie(cookie_header: &str, key: &Key) -> crate::Result<Self> {
        let mut jar = CookieJar::new();

        for part in cookie_header.split(';') {
            let trimmed = part.trim();
            if let Ok(cookie) = Cookie::parse(trimmed) {
                jar.add_original(cookie.into_owned());
            }
        }

        let verified = jar
            .signed(key)
            .get(OAUTH_COOKIE_NAME)
            .ok_or_else(|| crate::Error::bad_request("invalid or missing OAuth state cookie"))?;

        let payload: serde_json::Value = serde_json::from_str(verified.value())
            .map_err(|e| crate::Error::bad_request(format!("invalid OAuth state: {e}")))?;

        Ok(Self {
            state_nonce: payload["state"]
                .as_str()
                .ok_or_else(|| crate::Error::bad_request("missing state nonce"))?
                .to_string(),
            pkce_verifier: payload["pkce_verifier"]
                .as_str()
                .ok_or_else(|| crate::Error::bad_request("missing PKCE verifier"))?
                .to_string(),
            provider: payload["provider"]
                .as_str()
                .ok_or_else(|| crate::Error::bad_request("missing provider"))?
                .to_string(),
        })
    }
}

impl<S> FromRequestParts<S> for OAuthState
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = crate::Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let key: std::sync::Arc<Key> = app_state
            .get::<Key>()
            .ok_or_else(|| crate::Error::internal("Key not registered in service registry"))?;

        let cookie_header = parts
            .headers
            .get(COOKIE)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| crate::Error::bad_request("missing OAuth state cookie"))?;

        Self::from_signed_cookie(cookie_header, &key)
    }
}

pub struct AuthorizationRequest {
    pub(crate) redirect_url: String,
    pub(crate) set_cookie_header: String,
}

impl IntoResponse for AuthorizationRequest {
    fn into_response(self) -> Response {
        let mut response = Redirect::to(&self.redirect_url).into_response();
        if let Ok(value) = self.set_cookie_header.parse() {
            response.headers_mut().insert(SET_COOKIE, value);
        }
        response
    }
}

/// Build a signed OAuth state cookie. Returns (set_cookie_header, state_nonce, pkce_verifier).
pub(crate) fn build_oauth_cookie(
    provider: &str,
    key: &Key,
    cookie_config: &CookieConfig,
) -> (String, String, String) {
    let state_nonce = generate_random_string(32);
    let pkce_verifier = generate_random_string(64);

    let payload = serde_json::json!({
        "state": state_nonce,
        "pkce_verifier": pkce_verifier,
        "provider": provider,
    });

    let mut jar = CookieJar::new();
    let mut cookie = Cookie::new(OAUTH_COOKIE_NAME, payload.to_string());
    cookie.set_path("/");
    cookie.set_http_only(cookie_config.http_only);
    cookie.set_secure(cookie_config.secure);
    cookie.set_max_age(cookie::time::Duration::seconds(OAUTH_COOKIE_MAX_AGE_SECS));
    cookie.set_same_site(match cookie_config.same_site.as_str() {
        "strict" => SameSite::Strict,
        "none" => SameSite::None,
        _ => SameSite::Lax,
    });

    jar.signed_mut(key).add(cookie);

    let set_cookie_header = jar
        .get(OAUTH_COOKIE_NAME)
        .map(|c| c.to_string())
        .unwrap_or_default();

    (set_cookie_header, state_nonce, pkce_verifier)
}

/// Generate a PKCE code challenge (S256) from the verifier.
pub(crate) fn pkce_challenge(verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(verifier.as_bytes());
    base64url_encode(&hash)
}

fn generate_random_string(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    rand::fill(&mut bytes[..]);
    base64url_encode(&bytes)
}

fn base64url_encode(bytes: &[u8]) -> String {
    crate::encoding::base64url::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    fn test_cookie_config() -> CookieConfig {
        CookieConfig {
            secret: "a".repeat(64),
            secure: false,
            http_only: true,
            same_site: "lax".to_string(),
        }
    }

    fn test_key() -> Key {
        crate::cookie::key_from_config(&test_cookie_config()).unwrap()
    }

    #[test]
    fn authorization_request_into_response_redirects() {
        let req = AuthorizationRequest {
            redirect_url: "https://accounts.google.com/o/oauth2/v2/auth?foo=bar".to_string(),
            set_cookie_header: "_oauth_state=signed_value; Path=/; HttpOnly; SameSite=Lax"
                .to_string(),
        };
        let response = req.into_response();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let cookie = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cookie.contains("_oauth_state="));
    }

    #[test]
    fn build_and_parse_oauth_cookie_roundtrip() {
        let key = test_key();
        let cookie_config = test_cookie_config();

        let (set_cookie_header, state_nonce, pkce_verifier) =
            build_oauth_cookie("google", &key, &cookie_config);

        assert!(set_cookie_header.contains("_oauth_state="));
        assert!(set_cookie_header.contains("HttpOnly"));
        assert!(!state_nonce.is_empty());
        assert!(!pkce_verifier.is_empty());

        let parsed = OAuthState::from_signed_cookie(&set_cookie_header, &key).unwrap();
        assert_eq!(parsed.provider(), "google");
        assert_eq!(parsed.state_nonce(), &state_nonce);
        assert_eq!(parsed.pkce_verifier(), &pkce_verifier);
    }

    #[test]
    fn parse_tampered_cookie_fails() {
        let key = test_key();
        let cookie_config = test_cookie_config();

        let (set_cookie_header, _, _) = build_oauth_cookie("google", &key, &cookie_config);

        let tampered = set_cookie_header.replace("_oauth_state=", "_oauth_state=tampered");
        assert!(OAuthState::from_signed_cookie(&tampered, &key).is_err());
    }

    #[test]
    fn cross_provider_state_detected() {
        let key = test_key();
        let cookie_config = test_cookie_config();

        let (set_cookie_header, _, _) = build_oauth_cookie("google", &key, &cookie_config);
        let parsed = OAuthState::from_signed_cookie(&set_cookie_header, &key).unwrap();
        assert_eq!(parsed.provider(), "google");
        assert_ne!(parsed.provider(), "github");
    }
}
