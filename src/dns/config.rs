use std::net::SocketAddr;

use serde::Deserialize;

use crate::error::{Error, Result};

fn default_txt_prefix() -> String {
    "_modo-verify".into()
}

fn default_timeout_ms() -> u64 {
    5000
}

#[derive(Debug, Clone, Deserialize)]
pub struct DnsConfig {
    pub nameserver: String,
    #[serde(default = "default_txt_prefix")]
    pub txt_prefix: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

impl DnsConfig {
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
