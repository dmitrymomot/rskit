use serde::{Deserialize, Serialize};

/// Normalized user profile returned after a successful OAuth exchange.
///
/// Fields common to all providers are promoted to top-level fields. The raw JSON response from the
/// provider is preserved in [`raw`](UserProfile::raw) for any provider-specific data your
/// application needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    /// Lowercase provider identifier (`"google"`, `"github"`, …).
    pub provider: String,
    /// The user's unique ID within the provider's system.
    pub provider_user_id: String,
    /// Primary email address.
    pub email: String,
    /// Whether the provider has verified this email address.
    pub email_verified: bool,
    /// Display name, if the provider returned one.
    pub name: Option<String>,
    /// URL of the user's avatar image, if available.
    pub avatar_url: Option<String>,
    /// Raw JSON response from the provider's user-info endpoint.
    pub raw: serde_json::Value,
}
