use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct OAuthConfig {
    pub google: Option<OAuthProviderConfig>,
    pub github: Option<OAuthProviderConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthProviderConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}
