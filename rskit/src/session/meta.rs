use crate::app::AppState;
use crate::config::AppConfig;
use crate::session::{compute_fingerprint, parse_device_name, parse_device_type};
use axum::extract::FromRequestParts;
use axum::extract::connect_info::ConnectInfo;
use axum::http::HeaderMap;
use axum::http::request::Parts;
use std::net::{IpAddr, SocketAddr};

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
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self::from_request_data(
            &parts.extensions,
            &parts.headers,
            &state.config,
        ))
    }
}

impl SessionMeta {
    /// Build SessionMeta from request extensions and headers with proper IP extraction.
    ///
    /// Uses `ConnectInfo<SocketAddr>` when available and validates proxy headers
    /// against `trusted_proxies` config.
    pub fn from_request_data(
        extensions: &axum::http::Extensions,
        headers: &HeaderMap,
        config: &AppConfig,
    ) -> Self {
        let connect_ip = extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip());
        let ip_address = extract_ip(headers, connect_ip, &config.trusted_proxies);
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

/// Extract client IP with ConnectInfo awareness and trusted proxy validation.
///
/// - If `connect_ip` is present and `trusted_proxies` is non-empty, only trust
///   proxy headers (X-Forwarded-For, X-Real-IP) when the connection comes from
///   a trusted proxy network.
/// - If `trusted_proxies` is empty, always trust proxy headers (backwards-compatible).
/// - Falls back to `connect_ip` or "unknown".
fn extract_ip(
    headers: &HeaderMap,
    connect_ip: Option<IpAddr>,
    trusted_proxies: &[ipnet::IpNet],
) -> String {
    // If we have a direct connection IP and trusted_proxies is configured,
    // only trust proxy headers when connection is from a trusted proxy.
    if let Some(ip) = connect_ip
        && !trusted_proxies.is_empty()
        && !trusted_proxies.iter().any(|net| net.contains(&ip))
    {
        // Connection NOT from trusted proxy — use socket IP directly
        return ip.to_string();
    }

    // Connection from trusted proxy (or no ConnectInfo / no trusted_proxies configured)
    // — parse proxy headers

    // X-Forwarded-For: client, proxy1, proxy2 — take first valid IP
    if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        && let Some(first) = forwarded.split(',').next()
    {
        let candidate = first.trim();
        if candidate.parse::<IpAddr>().is_ok() {
            return candidate.to_string();
        }
    }

    // X-Real-IP: single IP from reverse proxy
    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let candidate = real_ip.trim();
        if candidate.parse::<IpAddr>().is_ok() {
            return candidate.to_string();
        }
    }

    // Final fallback: use connect_ip if available
    connect_ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn extract_ip_from_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        assert_eq!(extract_ip(&headers, None, &[]), "1.2.3.4");
    }

    #[test]
    fn extract_ip_from_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        assert_eq!(extract_ip(&headers, None, &[]), "9.8.7.6");
    }

    #[test]
    fn extract_ip_prefers_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        assert_eq!(extract_ip(&headers, None, &[]), "1.2.3.4");
    }

    #[test]
    fn extract_ip_falls_back_to_unknown() {
        let headers = HeaderMap::new();
        assert_eq!(extract_ip(&headers, None, &[]), "unknown");
    }

    #[test]
    fn extract_ip_falls_back_to_connect_ip() {
        let headers = HeaderMap::new();
        let connect_ip: IpAddr = "192.168.1.1".parse().unwrap();
        assert_eq!(extract_ip(&headers, Some(connect_ip), &[]), "192.168.1.1");
    }

    #[test]
    fn untrusted_source_ignores_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        let connect_ip: IpAddr = "10.0.0.50".parse().unwrap();
        let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/24".parse().unwrap()];
        // connect_ip IS in trusted range — XFF is trusted
        assert_eq!(extract_ip(&headers, Some(connect_ip), &trusted), "1.2.3.4");

        // connect_ip NOT in trusted range — XFF is ignored
        let untrusted_ip: IpAddr = "203.0.113.5".parse().unwrap();
        assert_eq!(
            extract_ip(&headers, Some(untrusted_ip), &trusted),
            "203.0.113.5"
        );
    }

    #[test]
    fn trusted_proxy_uses_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "8.8.8.8".parse().unwrap());
        let connect_ip: IpAddr = "10.0.0.1".parse().unwrap();
        let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/24".parse().unwrap()];
        assert_eq!(extract_ip(&headers, Some(connect_ip), &trusted), "8.8.8.8");
    }

    #[test]
    fn session_meta_from_request_data() {
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

        let extensions = axum::http::Extensions::default();
        let config = crate::config::AppConfig::default();
        let meta = SessionMeta::from_request_data(&extensions, &headers, &config);
        assert_eq!(meta.ip_address, "10.0.0.1");
        assert_eq!(meta.device_name, "Chrome on macOS");
        assert_eq!(meta.device_type, "desktop");
        assert_eq!(meta.fingerprint.len(), 64);
    }
}
