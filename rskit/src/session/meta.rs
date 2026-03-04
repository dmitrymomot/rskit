use crate::app::AppState;
use crate::session::{compute_fingerprint, parse_device_name, parse_device_type};
use axum::extract::FromRequestParts;
use axum::http::HeaderMap;
use axum::http::request::Parts;

/// Request metadata used to create sessions.
///
/// Implements `FromRequestParts` so it can be used as a handler parameter
/// alongside body-consuming extractors like `Form` or `Json`.
#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub ip_address: String,
    pub user_agent: String,
    pub device_name: String,
    pub device_type: String,
    pub fingerprint: String,
}

impl FromRequestParts<AppState> for SessionMeta {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self::from_headers(&parts.headers))
    }
}

impl SessionMeta {
    /// Build SessionMeta directly from headers. Used by both the extractor
    /// and the session middleware.
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let ip_address = extract_ip(headers);
        let user_agent = header_value(headers, "user-agent");
        let accept_language = header_value(headers, "accept-language");
        let accept_encoding = header_value(headers, "accept-encoding");

        let device_name = parse_device_name(&user_agent);
        let device_type = parse_device_type(&user_agent);
        let fingerprint = compute_fingerprint(&user_agent, &accept_language, &accept_encoding);

        Self {
            ip_address,
            user_agent,
            device_name,
            device_type,
            fingerprint,
        }
    }
}

fn header_value(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

/// Extract client IP from proxy headers, falling back to "unknown".
fn extract_ip(headers: &HeaderMap) -> String {
    // X-Forwarded-For: client, proxy1, proxy2 — take first
    if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        && let Some(first) = forwarded.split(',').next()
    {
        let ip = first.trim();
        if !ip.is_empty() {
            return ip.to_string();
        }
    }

    // X-Real-IP: single IP from reverse proxy
    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let ip = real_ip.trim();
        if !ip.is_empty() {
            return ip.to_string();
        }
    }

    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn extract_ip_from_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        assert_eq!(extract_ip(&headers), "1.2.3.4");
    }

    #[test]
    fn extract_ip_from_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        assert_eq!(extract_ip(&headers), "9.8.7.6");
    }

    #[test]
    fn extract_ip_prefers_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        assert_eq!(extract_ip(&headers), "1.2.3.4");
    }

    #[test]
    fn extract_ip_falls_back_to_unknown() {
        let headers = HeaderMap::new();
        assert_eq!(extract_ip(&headers), "unknown");
    }

    #[test]
    fn session_meta_from_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "10.0.0.1".parse().unwrap());
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0.0.0"
                .parse()
                .unwrap(),
        );
        headers.insert("accept-language", "en-US".parse().unwrap());
        headers.insert("accept-encoding", "gzip".parse().unwrap());

        let meta = SessionMeta::from_headers(&headers);
        assert_eq!(meta.ip_address, "10.0.0.1");
        assert_eq!(meta.device_name, "Chrome on macOS");
        assert_eq!(meta.device_type, "desktop");
        assert_eq!(meta.fingerprint.len(), 64);
    }
}
