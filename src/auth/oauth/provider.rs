use std::future::Future;

use super::{
    config::CallbackParams,
    profile::UserProfile,
    state::{AuthorizationRequest, OAuthState},
};

pub trait OAuthProvider: Send + Sync {
    fn name(&self) -> &str;
    fn authorize_url(&self) -> crate::Result<AuthorizationRequest>;
    fn exchange(
        &self,
        params: &CallbackParams,
        state: &OAuthState,
    ) -> impl Future<Output = crate::Result<UserProfile>> + Send;
}
