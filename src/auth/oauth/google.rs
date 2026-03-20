use axum_extra::extract::cookie::Key;

use crate::cookie::CookieConfig;

use super::{
    config::{CallbackParams, OAuthProviderConfig},
    profile::UserProfile,
    provider::OAuthProvider,
    state::{AuthorizationRequest, OAuthState},
};

#[allow(dead_code)]
pub struct Google {
    config: OAuthProviderConfig,
    cookie_config: CookieConfig,
    key: Key,
}

impl Google {
    pub fn new(config: &OAuthProviderConfig, cookie_config: &CookieConfig, key: &Key) -> Self {
        Self {
            config: config.clone(),
            cookie_config: cookie_config.clone(),
            key: key.clone(),
        }
    }
}

impl OAuthProvider for Google {
    fn name(&self) -> &str {
        "google"
    }
    fn authorize_url(&self) -> crate::Result<AuthorizationRequest> {
        todo!()
    }
    async fn exchange(
        &self,
        _params: &CallbackParams,
        _state: &OAuthState,
    ) -> crate::Result<UserProfile> {
        todo!()
    }
}
