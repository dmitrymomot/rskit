use http::HeaderMap;

use super::device::{parse_device_name, parse_device_type};
use super::fingerprint::compute_fingerprint;

#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,
    pub device_type: String,
    pub fingerprint: String,
}

impl SessionMeta {
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

pub fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}
