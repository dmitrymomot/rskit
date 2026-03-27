use http::HeaderMap;

use super::device::{parse_device_name, parse_device_type};
use super::fingerprint::compute_fingerprint;

/// Metadata derived from request headers at the time a session is created.
///
/// Stored alongside the session row and used for fingerprint validation on
/// subsequent requests.
#[derive(Debug, Clone)]
pub struct SessionMeta {
    /// Client IP address (from `ClientIp` or `ConnectInfo`).
    pub ip_address: String,
    /// Raw `User-Agent` header value.
    pub user_agent: String,
    /// Human-readable device name, e.g. `"Chrome on macOS"`.
    pub device_name: String,
    /// Device category: `"desktop"`, `"mobile"`, or `"tablet"`.
    pub device_type: String,
    /// SHA-256 fingerprint derived from user-agent, accept-language, and
    /// accept-encoding headers.
    pub fingerprint: String,
}

impl SessionMeta {
    /// Build `SessionMeta` from parsed header strings.
    pub fn from_headers(
        ip_address: String,
        user_agent: &str,
        accept_language: &str,
        accept_encoding: &str,
    ) -> Self {
        Self {
            ip_address,
            device_name: parse_device_name(user_agent),
            device_type: parse_device_type(user_agent),
            fingerprint: compute_fingerprint(user_agent, accept_language, accept_encoding),
            user_agent: user_agent.to_string(),
        }
    }
}

/// Extract a header value as a string slice, returning `""` when absent or
/// non-UTF-8.
pub fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}
