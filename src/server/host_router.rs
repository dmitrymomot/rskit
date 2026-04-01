use http::request::Parts;

use crate::Error;

/// Resolve the effective host from a request, checking proxy headers first.
///
/// Checks in order:
/// 1. `Forwarded` header (RFC 7239) — `host=` directive
/// 2. `X-Forwarded-Host` header
/// 3. `Host` header
///
/// After extraction the value is lowercased and any trailing port is stripped.
#[cfg_attr(not(test), expect(dead_code))]
fn resolve_host(parts: &Parts) -> Result<String, Error> {
    // 1. Forwarded: host=...
    if let Some(fwd) = parts.headers.get("forwarded")
        && let Ok(fwd_str) = fwd.to_str()
    {
        // Parse "host=" directive from the first element.
        // The Forwarded header can have comma-separated entries for multiple hops;
        // only the first element is relevant.
        // Format: "for=...; host=example.com; proto=https"
        let first_element = fwd_str.split(',').next().unwrap_or(fwd_str);
        for directive in first_element.split(';') {
            let directive = directive.trim();
            if let Some(host) = directive.strip_prefix("host=") {
                let host = host.trim();
                if !host.is_empty() {
                    return Ok(strip_port(host).to_lowercase());
                }
            }
        }
    }

    // 2. X-Forwarded-Host
    if let Some(xfh) = parts.headers.get("x-forwarded-host")
        && let Ok(host) = xfh.to_str()
    {
        let host = host.trim();
        if !host.is_empty() {
            return Ok(strip_port(host).to_lowercase());
        }
    }

    // 3. Host header
    if let Some(h) = parts.headers.get(http::header::HOST)
        && let Ok(host) = h.to_str()
    {
        let host = host.trim();
        if !host.is_empty() {
            return Ok(strip_port(host).to_lowercase());
        }
    }

    Err(Error::bad_request("missing or invalid Host header"))
}

/// Strip an optional `:port` suffix from a host string.
fn strip_port(host: &str) -> &str {
    match host.rfind(':') {
        Some(pos) if host[pos + 1..].bytes().all(|b| b.is_ascii_digit()) => &host[..pos],
        _ => host,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts_with_headers(headers: &[(&str, &str)]) -> Parts {
        let mut builder = http::Request::builder();
        for &(name, value) in headers {
            builder = builder.header(name, value);
        }
        let (parts, _) = builder.body(()).unwrap().into_parts();
        parts
    }

    #[test]
    fn resolve_from_host_header() {
        let parts = parts_with_headers(&[("host", "acme.com")]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_strips_port() {
        let parts = parts_with_headers(&[("host", "acme.com:8080")]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_lowercases() {
        let parts = parts_with_headers(&[("host", "ACME.COM")]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_x_forwarded_host_over_host() {
        let parts =
            parts_with_headers(&[("host", "proxy.internal"), ("x-forwarded-host", "acme.com")]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_forwarded_over_x_forwarded_host() {
        let parts = parts_with_headers(&[
            ("host", "proxy.internal"),
            ("x-forwarded-host", "xfh.com"),
            ("forwarded", "for=1.2.3.4; host=acme.com; proto=https"),
        ]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_forwarded_strips_port() {
        let parts = parts_with_headers(&[("forwarded", "host=acme.com:443")]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_x_forwarded_host_strips_port() {
        let parts = parts_with_headers(&[("x-forwarded-host", "acme.com:8080")]);
        assert_eq!(resolve_host(&parts).unwrap(), "acme.com");
    }

    #[test]
    fn resolve_missing_all_headers_returns_400() {
        let parts = parts_with_headers(&[]);
        let err = resolve_host(&parts).unwrap_err();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn resolve_forwarded_without_host_falls_through() {
        let parts = parts_with_headers(&[
            ("forwarded", "for=1.2.3.4; proto=https"),
            ("host", "fallback.com"),
        ]);
        assert_eq!(resolve_host(&parts).unwrap(), "fallback.com");
    }
}
