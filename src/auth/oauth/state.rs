use axum::response::{IntoResponse, Redirect, Response};
use http::header::SET_COOKIE;

#[allow(dead_code)]
pub struct OAuthState {
    state_nonce: String,
    pkce_verifier: String,
    provider: String,
}

#[allow(dead_code)]
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
