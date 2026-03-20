use http::HeaderMap;
use std::net::IpAddr;

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

pub fn extract_client_ip(
    headers: &HeaderMap,
    trusted_proxies: &[String],
    connect_ip: Option<IpAddr>,
) -> String {
    let parsed_nets: Vec<ipnet::IpNet> = trusted_proxies
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    if let Some(ip) = connect_ip
        && !parsed_nets.is_empty()
        && !parsed_nets.iter().any(|net| net.contains(&ip))
    {
        return ip.to_string();
    }

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
