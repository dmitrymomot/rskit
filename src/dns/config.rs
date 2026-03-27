//! Configuration for the DNS verification module.

use std::net::SocketAddr;

use serde::Deserialize;

use crate::error::{Error, Result};

fn default_txt_prefix() -> String {
    "_modo-verify".into()
}

fn default_timeout_ms() -> u64 {
    5000
}

/// Configuration for [`super::DomainVerifier`].
///
/// Deserializes from YAML via `serde`. The `txt_prefix` and `timeout_ms`
/// fields have defaults and can be omitted.
///
/// # Example (YAML)
///
/// ```yaml
/// dns:
///   nameserver: "8.8.8.8:53"
///   txt_prefix: "_myapp-verify"   # default: _modo-verify
///   timeout_ms: 5000              # default: 5000
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DnsConfig {
    /// Nameserver address, with or without port. Port 53 is appended when omitted.
    ///
    /// Examples: `"8.8.8.8:53"`, `"1.1.1.1"`.
    pub nameserver: String,
    /// Prefix prepended to the domain when looking up TXT records.
    ///
    /// The resolved TXT lookup name is `{txt_prefix}.{domain}`.
    /// Defaults to `"_modo-verify"`.
    #[serde(default = "default_txt_prefix")]
    pub txt_prefix: String,
    /// UDP receive timeout in milliseconds. Defaults to `5000`.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            nameserver: "8.8.8.8".into(),
            txt_prefix: "_modo-verify".into(),
            timeout_ms: 5000,
        }
    }
}

impl DnsConfig {
    /// Create a DNS configuration with the given nameserver address.
    ///
    /// Defaults: `txt_prefix = "_modo-verify"`, `timeout_ms = 5000`.
    pub fn new(nameserver: impl Into<String>) -> Self {
        Self {
            nameserver: nameserver.into(),
            txt_prefix: "_modo-verify".into(),
            timeout_ms: 5000,
        }
    }

    /// Parse `nameserver` into a [`SocketAddr`].
    ///
    /// If the address already contains a port it is used as-is; otherwise port
    /// `53` is appended. Returns [`crate::Error`] with status 500 when the
    /// address is not a valid IP or hostname+port.
    pub fn parse_nameserver(&self) -> Result<SocketAddr> {
        if let Ok(addr) = self.nameserver.parse::<SocketAddr>() {
            return Ok(addr);
        }
        let with_port = format!("{}:53", self.nameserver);
        with_port.parse::<SocketAddr>().map_err(|_| {
            Error::internal(format!(
                "invalid dns nameserver address: {}",
                self.nameserver
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_config() {
        let yaml = r#"
nameserver: "8.8.8.8:53"
txt_prefix: "_myapp-verify"
timeout_ms: 3000
"#;
        let config: DnsConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.nameserver, "8.8.8.8:53");
        assert_eq!(config.txt_prefix, "_myapp-verify");
        assert_eq!(config.timeout_ms, 3000);
    }

    #[test]
    fn defaults_applied_when_fields_omitted() {
        let yaml = r#"
nameserver: "8.8.8.8"
"#;
        let config: DnsConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.nameserver, "8.8.8.8");
        assert_eq!(config.txt_prefix, "_modo-verify");
        assert_eq!(config.timeout_ms, 5000);
    }

    #[test]
    fn parse_nameserver_with_port() {
        let config = DnsConfig {
            nameserver: "1.1.1.1:53".into(),
            txt_prefix: "_modo-verify".into(),
            timeout_ms: 5000,
        };
        let addr = config.parse_nameserver().unwrap();
        assert_eq!(addr.to_string(), "1.1.1.1:53");
    }

    #[test]
    fn parse_nameserver_without_port_appends_53() {
        let config = DnsConfig {
            nameserver: "8.8.8.8".into(),
            txt_prefix: "_modo-verify".into(),
            timeout_ms: 5000,
        };
        let addr = config.parse_nameserver().unwrap();
        assert_eq!(addr.to_string(), "8.8.8.8:53");
    }

    #[test]
    fn parse_nameserver_invalid_address_fails() {
        let config = DnsConfig {
            nameserver: "not-an-address".into(),
            txt_prefix: "_modo-verify".into(),
            timeout_ms: 5000,
        };
        let err = config.parse_nameserver().unwrap_err();
        assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    }
}
