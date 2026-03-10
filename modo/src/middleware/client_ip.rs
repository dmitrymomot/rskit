use crate::app::AppState;
use crate::error::Error;
use axum::extract::FromRequestParts;
use axum::extract::{ConnectInfo, State};
use axum::http::Request;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use std::net::{IpAddr, SocketAddr};

/// Pre-parsed trusted proxy CIDR ranges, registered as a service at startup.
#[derive(Debug)]
pub(crate) struct TrustedProxies(pub Vec<CidrRange>);

/// The resolved client IP address, inserted into request extensions.
#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub IpAddr);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<ClientIp>()
            .copied()
            .ok_or_else(|| Error::internal("ClientIp not found in request extensions"))
    }
}

pub async fn client_ip_middleware(
    State(state): State<AppState>,
    connect_info: ConnectInfo<SocketAddr>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let trusted_arc = state.services.get::<TrustedProxies>();
    let trusted: &[CidrRange] = trusted_arc.as_ref().map(|t| t.0.as_slice()).unwrap_or(&[]);
    let socket_ip = connect_info.0.ip();

    let (mut parts, body) = request.into_parts();

    let ip = resolve_client_ip(&parts.headers, socket_ip, trusted);
    parts.extensions.insert(ClientIp(ip));

    let request = Request::from_parts(parts, body);
    next.run(request).await
}

fn resolve_client_ip(
    headers: &axum::http::HeaderMap,
    socket_ip: IpAddr,
    trusted: &[CidrRange],
) -> IpAddr {
    // Only inspect proxy headers when the direct connection is from a trusted proxy
    if !is_trusted(socket_ip, trusted) {
        return socket_ip;
    }

    // 1. CF-Connecting-IP (Cloudflare)
    if let Some(ip) = header_ip(headers, "cf-connecting-ip") {
        return ip;
    }

    // 2. X-Real-IP
    if let Some(ip) = header_ip(headers, "x-real-ip") {
        return ip;
    }

    // 3. X-Forwarded-For: rightmost untrusted
    if let Some(val) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        let addrs: Vec<&str> = val.split(',').map(|s| s.trim()).collect();
        // Iterate right to left, return first IP not in trusted_proxies
        for raw in addrs.iter().rev() {
            if let Ok(ip) = raw.parse::<IpAddr>()
                && !is_trusted(ip, trusted)
            {
                return ip;
            }
        }
        // All are trusted — use leftmost
        if let Some(first) = addrs.first()
            && let Ok(ip) = first.parse::<IpAddr>()
        {
            return ip;
        }
    }

    // 4. Fallback to socket address
    socket_ip
}

fn header_ip(headers: &axum::http::HeaderMap, name: &str) -> Option<IpAddr> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse().ok())
}

fn is_trusted(ip: IpAddr, trusted: &[CidrRange]) -> bool {
    trusted.iter().any(|cidr| cidr.contains(ip))
}

pub(crate) fn parse_trusted_proxies(proxies: &[String]) -> Vec<CidrRange> {
    proxies.iter().filter_map(|s| CidrRange::parse(s)).collect()
}

// ---------------------------------------------------------------------------
// CIDR matching
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) struct CidrRange {
    network: IpAddr,
    prefix_len: u8,
}

impl CidrRange {
    fn parse(s: &str) -> Option<Self> {
        if let Some((addr_str, prefix_str)) = s.split_once('/') {
            let network: IpAddr = addr_str.parse().ok()?;
            let prefix_len: u8 = prefix_str.parse().ok()?;
            let max = match network {
                IpAddr::V4(_) => 32,
                IpAddr::V6(_) => 128,
            };
            if prefix_len > max {
                return None;
            }
            Some(Self {
                network,
                prefix_len,
            })
        } else {
            // Single IP — treat as /32 or /128
            let network: IpAddr = s.parse().ok()?;
            let prefix_len = match network {
                IpAddr::V4(_) => 32,
                IpAddr::V6(_) => 128,
            };
            Some(Self {
                network,
                prefix_len,
            })
        }
    }

    fn contains(&self, ip: IpAddr) -> bool {
        match (self.network, ip) {
            (IpAddr::V4(net), IpAddr::V4(target)) => {
                if self.prefix_len == 0 {
                    return true;
                }
                let net_bits = u32::from(net);
                let target_bits = u32::from(target);
                let mask = u32::MAX << (32 - self.prefix_len);
                (net_bits & mask) == (target_bits & mask)
            }
            (IpAddr::V6(net), IpAddr::V6(target)) => {
                if self.prefix_len == 0 {
                    return true;
                }
                let net_bits = u128::from(net);
                let target_bits = u128::from(target);
                let mask = u128::MAX << (128 - self.prefix_len);
                (net_bits & mask) == (target_bits & mask)
            }
            _ => false, // v4/v6 mismatch
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cidr_parse_v4() {
        let cidr = CidrRange::parse("10.0.0.0/8").unwrap();
        assert!(cidr.contains("10.1.2.3".parse().unwrap()));
        assert!(cidr.contains("10.255.255.255".parse().unwrap()));
        assert!(!cidr.contains("11.0.0.1".parse().unwrap()));
    }

    #[test]
    fn test_cidr_parse_v4_single() {
        let cidr = CidrRange::parse("192.168.1.1").unwrap();
        assert!(cidr.contains("192.168.1.1".parse().unwrap()));
        assert!(!cidr.contains("192.168.1.2".parse().unwrap()));
    }

    #[test]
    fn test_cidr_parse_v6() {
        let cidr = CidrRange::parse("::1/128").unwrap();
        assert!(cidr.contains("::1".parse().unwrap()));
        assert!(!cidr.contains("::2".parse().unwrap()));
    }

    #[test]
    fn test_cidr_v4_v6_mismatch() {
        let cidr = CidrRange::parse("10.0.0.0/8").unwrap();
        assert!(!cidr.contains("::1".parse().unwrap()));
    }

    #[test]
    fn test_cidr_invalid() {
        assert!(CidrRange::parse("10.0.0.0/33").is_none());
        assert!(CidrRange::parse("not-an-ip").is_none());
    }

    #[test]
    fn test_resolve_xff_rightmost_untrusted() {
        let trusted = vec![CidrRange::parse("10.0.0.0/8").unwrap()];
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.1, 10.0.0.1, 10.0.0.2".parse().unwrap(),
        );
        let socket: IpAddr = "10.0.0.3".parse().unwrap();
        let ip = resolve_client_ip(&headers, socket, &trusted);
        assert_eq!(ip, "203.0.113.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn test_resolve_cf_connecting_ip() {
        let trusted = vec![CidrRange::parse("127.0.0.1/32").unwrap()];
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("cf-connecting-ip", "1.2.3.4".parse().unwrap());
        headers.insert("x-forwarded-for", "5.6.7.8".parse().unwrap());
        let socket: IpAddr = "127.0.0.1".parse().unwrap();
        let ip = resolve_client_ip(&headers, socket, &trusted);
        assert_eq!(ip, "1.2.3.4".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn test_resolve_fallback_to_socket() {
        let trusted = vec![];
        let headers = axum::http::HeaderMap::new();
        let socket: IpAddr = "192.168.0.1".parse().unwrap();
        let ip = resolve_client_ip(&headers, socket, &trusted);
        assert_eq!(ip, socket);
    }

    #[test]
    fn test_cf_connecting_ip_ignored_when_socket_untrusted() {
        let trusted = vec![CidrRange::parse("10.0.0.0/8").unwrap()];
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("cf-connecting-ip", "1.2.3.4".parse().unwrap());
        let socket: IpAddr = "203.0.113.50".parse().unwrap();
        let ip = resolve_client_ip(&headers, socket, &trusted);
        assert_eq!(ip, socket);
    }

    #[test]
    fn test_x_real_ip_ignored_when_socket_untrusted() {
        let trusted = vec![CidrRange::parse("10.0.0.0/8").unwrap()];
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "1.2.3.4".parse().unwrap());
        let socket: IpAddr = "203.0.113.50".parse().unwrap();
        let ip = resolve_client_ip(&headers, socket, &trusted);
        assert_eq!(ip, socket);
    }

    #[test]
    fn test_x_real_ip_used_when_socket_trusted() {
        let trusted = vec![CidrRange::parse("127.0.0.0/8").unwrap()];
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "203.0.113.1".parse().unwrap());
        let socket: IpAddr = "127.0.0.1".parse().unwrap();
        let ip = resolve_client_ip(&headers, socket, &trusted);
        assert_eq!(ip, "203.0.113.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn test_xff_ignored_when_socket_untrusted() {
        let trusted = vec![CidrRange::parse("10.0.0.0/8").unwrap()];
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 10.0.0.1".parse().unwrap());
        let socket: IpAddr = "203.0.113.50".parse().unwrap();
        let ip = resolve_client_ip(&headers, socket, &trusted);
        assert_eq!(ip, socket);
    }
}
