use http::HeaderMap;
use modo::session::meta::{SessionMeta, extract_client_ip, header_str};
use std::net::IpAddr;

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
fn header_str_returns_value() {
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", "test-ua".parse().unwrap());
    assert_eq!(header_str(&headers, "user-agent"), "test-ua");
}

#[test]
fn header_str_returns_empty_for_missing() {
    let headers = HeaderMap::new();
    assert_eq!(header_str(&headers, "user-agent"), "");
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
