use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub provider: String,
    pub provider_user_id: String,
    pub email: String,
    pub email_verified: bool,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub raw: serde_json::Value,
}
