use crate::device::{parse_device_name, parse_device_type};
use crate::fingerprint::compute_fingerprint;
use http::HeaderMap;
use std::net::IpAddr;

/// Request metadata used to create sessions.
/// Built by the middleware from request headers.
#[derive(Debug, Clone)]
pub struct SessionMeta {
    /// Client IP address (respects trusted-proxy configuration).
    pub ip_address: String,
    /// Raw `User-Agent` header value.
    pub user_agent: String,
    /// Human-readable device name (e.g. `"Chrome on macOS"`).
    pub device_name: String,
    /// Device category: `"desktop"`, `"mobile"`, or `"tablet"`.
    pub device_type: String,
    /// SHA-256 fingerprint used for hijack detection.
    pub fingerprint: String,
}

impl SessionMeta {
    /// Build `SessionMeta` from individual header values.
    ///
    /// `ip_address` should already be the resolved client IP (use
    /// [`extract_client_ip`] to obtain it from raw headers).
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

/// Return the value of a named header as a `&str`, or `""` if absent or
/// non-ASCII.
pub fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

/// Extract client IP with trusted proxy validation.
///
/// # Security
///
/// When `trusted_proxies` is empty (the default), proxy headers
/// (`X-Forwarded-For`, `X-Real-IP`) are trusted unconditionally. Any client
/// can spoof their IP address by setting these headers. In production behind a
/// reverse proxy, **always** configure `trusted_proxies` to your proxy's CIDR
/// range (e.g., `["10.0.0.0/8"]`). Without a reverse proxy, set a dummy value
/// like `["127.0.0.1/32"]` to ignore proxy headers entirely.
///
/// When `trusted_proxies` is non-empty, proxy headers are only trusted when
/// `connect_ip` originates from a listed CIDR range. Otherwise the raw
/// `connect_ip` is returned.
pub fn extract_client_ip(
    headers: &HeaderMap,
    trusted_proxies: &[String],
    connect_ip: Option<IpAddr>,
) -> String {
    let parsed_nets: Vec<ipnet::IpNet> = trusted_proxies
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    // If we have a direct connection IP and trusted_proxies is configured,
    // only trust proxy headers when connection is from a trusted proxy.
    if let Some(ip) = connect_ip
        && !parsed_nets.is_empty()
        && !parsed_nets.iter().any(|net| net.contains(&ip))
    {
        return ip.to_string();
    }

    // Connection from trusted proxy (or no ConnectInfo / no trusted_proxies configured)
    if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        && let Some(first) = forwarded.split(',').next()
    {
        let candidate = first.trim();
        if candidate.parse::<IpAddr>().is_ok() {
            return candidate.to_string();
        }
    }

    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let candidate = real_ip.trim();
        if candidate.parse::<IpAddr>().is_ok() {
            return candidate.to_string();
        }
    }

    connect_ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_ip_from_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        assert_eq!(extract_client_ip(&headers, &[], None), "1.2.3.4");
    }

    #[test]
    fn extract_ip_from_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        assert_eq!(extract_client_ip(&headers, &[], None), "9.8.7.6");
    }

    #[test]
    fn extract_ip_prefers_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        assert_eq!(extract_client_ip(&headers, &[], None), "1.2.3.4");
    }

    #[test]
    fn extract_ip_falls_back_to_unknown() {
        let headers = HeaderMap::new();
        assert_eq!(extract_client_ip(&headers, &[], None), "unknown");
    }

    #[test]
    fn extract_ip_falls_back_to_connect_ip() {
        let headers = HeaderMap::new();
        let ip: IpAddr = "192.168.1.1".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, &[], Some(ip)), "192.168.1.1");
    }

    #[test]
    fn untrusted_source_ignores_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        let untrusted: IpAddr = "203.0.113.5".parse().unwrap();
        let trusted = vec!["10.0.0.0/24".to_string()];
        assert_eq!(
            extract_client_ip(&headers, &trusted, Some(untrusted)),
            "203.0.113.5"
        );
    }

    #[test]
    fn trusted_proxy_uses_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "8.8.8.8".parse().unwrap());
        let trusted_ip: IpAddr = "10.0.0.1".parse().unwrap();
        let trusted = vec!["10.0.0.0/24".to_string()];
        assert_eq!(
            extract_client_ip(&headers, &trusted, Some(trusted_ip)),
            "8.8.8.8"
        );
    }

    #[test]
    fn session_meta_from_headers() {
        let meta = SessionMeta::from_headers(
            "10.0.0.1".to_string(),
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0.0.0",
            "en-US",
            "gzip",
        );
        assert_eq!(meta.ip_address, "10.0.0.1");
        assert_eq!(meta.device_name, "Chrome on macOS");
        assert_eq!(meta.device_type, "desktop");
        assert_eq!(meta.fingerprint.len(), 64);
    }
}
