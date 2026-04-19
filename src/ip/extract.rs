use http::HeaderMap;
use std::net::{IpAddr, Ipv4Addr};

/// Resolve the real client IP from headers and connection info.
///
/// Resolution order:
/// 1. If `trusted_proxies` is non-empty, `connect_ip` is `Some`, and
///    `connect_ip` is NOT contained in any trusted range → return `connect_ip`
///    (the peer connected directly, so proxy headers must be ignored to avoid
///    spoofing).
/// 2. `X-Forwarded-For` → leftmost entry that parses as an [`IpAddr`].
/// 3. `X-Real-IP` → value parsed as an [`IpAddr`].
/// 4. `connect_ip` if provided.
/// 5. `127.0.0.1` as final fallback.
///
/// This is the low-level primitive used by [`ClientIpLayer`](crate::ip::ClientIpLayer);
/// prefer the layer in HTTP handlers and call this directly only when you
/// already have a [`HeaderMap`] without a Tower stack.
pub fn extract_client_ip(
    headers: &HeaderMap,
    trusted_proxies: &[ipnet::IpNet],
    connect_ip: Option<IpAddr>,
) -> IpAddr {
    if let Some(ip) = connect_ip
        && !trusted_proxies.is_empty()
        && !trusted_proxies.iter().any(|net| net.contains(&ip))
    {
        return ip;
    }

    if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        && let Some(first) = forwarded.split(',').next()
    {
        let candidate = first.trim();
        if let Ok(ip) = candidate.parse::<IpAddr>() {
            return ip;
        }
    }

    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let candidate = real_ip.trim();
        if let Ok(ip) = candidate.parse::<IpAddr>() {
            return ip;
        }
    }

    connect_ip.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn direct_ip_not_in_trusted_proxies() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        let connect_ip: IpAddr = "203.0.113.5".parse().unwrap();
        let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/24".parse().unwrap()];
        assert_eq!(
            extract_client_ip(&headers, &trusted, Some(connect_ip)),
            connect_ip
        );
    }

    #[test]
    fn trusted_proxy_uses_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "8.8.8.8, 10.0.0.1".parse().unwrap());
        let connect_ip: IpAddr = "10.0.0.1".parse().unwrap();
        let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/24".parse().unwrap()];
        let expected: IpAddr = "8.8.8.8".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, &trusted, Some(connect_ip)),
            expected
        );
    }

    #[test]
    fn trusted_proxy_uses_x_real_ip_when_no_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        let connect_ip: IpAddr = "10.0.0.1".parse().unwrap();
        let trusted: Vec<ipnet::IpNet> = vec!["10.0.0.0/24".parse().unwrap()];
        let expected: IpAddr = "9.8.7.6".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, &trusted, Some(connect_ip)),
            expected
        );
    }

    #[test]
    fn no_trusted_proxies_uses_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        let expected: IpAddr = "1.2.3.4".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, &[], None), expected);
    }

    #[test]
    fn no_trusted_proxies_uses_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        let expected: IpAddr = "9.8.7.6".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, &[], None), expected);
    }

    #[test]
    fn xff_preferred_over_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        let expected: IpAddr = "1.2.3.4".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, &[], None), expected);
    }

    #[test]
    fn fallback_to_connect_ip() {
        let headers = HeaderMap::new();
        let connect_ip: IpAddr = "192.168.1.1".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, &[], Some(connect_ip)),
            connect_ip
        );
    }

    #[test]
    fn fallback_to_localhost() {
        let headers = HeaderMap::new();
        assert_eq!(
            extract_client_ip(&headers, &[], None),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        );
    }

    #[test]
    fn invalid_xff_falls_back() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "not-an-ip".parse().unwrap());
        let connect_ip: IpAddr = "192.168.1.1".parse().unwrap();
        assert_eq!(
            extract_client_ip(&headers, &[], Some(connect_ip)),
            connect_ip
        );
    }

    #[test]
    fn empty_trusted_proxies_with_connect_ip_trusts_xff() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        let connect_ip: IpAddr = "203.0.113.5".parse().unwrap();
        let expected: IpAddr = "1.2.3.4".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, &[], Some(connect_ip)), expected);
    }
}
