//! Token pair returned by JWT session lifecycle methods.

use serde::{Deserialize, Serialize};

/// A pair of JWT tokens issued on successful authentication or rotation.
///
/// The `access_token` is short-lived and used to authenticate API requests.
/// The `refresh_token` is long-lived and used to obtain a new token pair via
/// the rotate endpoint. Both `*_expires_at` fields are Unix timestamps in seconds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    /// Short-lived token used to authenticate API requests.
    pub access_token: String,
    /// Long-lived token used to rotate the pair without re-authenticating.
    pub refresh_token: String,
    /// Unix timestamp (seconds) when the access token expires.
    pub access_expires_at: u64,
    /// Unix timestamp (seconds) when the refresh token expires.
    pub refresh_expires_at: u64,
}
