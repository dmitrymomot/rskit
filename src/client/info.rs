use axum::extract::FromRequestParts;
use http::{HeaderMap, request::Parts};

use crate::error::Error;
use crate::ip::ClientIp;

use super::device::{parse_device_name, parse_device_type};
use super::fingerprint::compute_fingerprint;

/// Client request context: IP, user-agent, parsed device fields, and a
/// server-computed browser fingerprint.
///
/// Implements [`FromRequestParts`] for automatic extraction in handlers.
/// Requires [`ClientIpLayer`](crate::ip::ClientIpLayer) for the `ip` field;
/// if the layer is absent, `ip` will be `None`.
///
/// For non-HTTP contexts (background jobs, CLI tools), use the builder:
///
/// ```
/// use modo::client::ClientInfo;
///
/// let info = ClientInfo::new()
///     .ip("1.2.3.4")
///     .user_agent("my-script/1.0");
/// ```
///
/// To populate device and fingerprint fields from request headers (e.g.
/// inside middleware that already holds a `&HeaderMap`), use
/// [`ClientInfo::from_headers`].
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
    ip: Option<String>,
    user_agent: Option<String>,
    device_name: Option<String>,
    device_type: Option<String>,
    fingerprint: Option<String>,
}

impl ClientInfo {
    /// Create an empty `ClientInfo` with all fields set to `None`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the client IP address.
    pub fn ip(mut self, ip: impl Into<String>) -> Self {
        self.ip = Some(ip.into());
        self
    }

    /// Set the user-agent string.
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// Set the parsed device name (e.g. `"Chrome on macOS"`).
    pub fn device_name(mut self, name: impl Into<String>) -> Self {
        self.device_name = Some(name.into());
        self
    }

    /// Set the device type (`"desktop"`, `"mobile"`, or `"tablet"`).
    pub fn device_type(mut self, kind: impl Into<String>) -> Self {
        self.device_type = Some(kind.into());
        self
    }

    /// Set the SHA-256 browser fingerprint.
    pub fn fingerprint(mut self, fp: impl Into<String>) -> Self {
        self.fingerprint = Some(fp.into());
        self
    }

    /// Build a fully-populated `ClientInfo` from the headers a server already
    /// has at hand.
    ///
    /// Parses `device_name` and `device_type` from `user_agent`, and computes
    /// the fingerprint from `user_agent + accept_language + accept_encoding`.
    /// An empty `user_agent` still yields meaningful values
    /// (`"Unknown on Unknown"` / `"desktop"` / a stable hash).
    pub fn from_headers(
        ip: Option<String>,
        user_agent: &str,
        accept_language: &str,
        accept_encoding: &str,
    ) -> Self {
        Self {
            ip,
            user_agent: Some(user_agent.to_string()),
            device_name: Some(parse_device_name(user_agent)),
            device_type: Some(parse_device_type(user_agent)),
            fingerprint: Some(compute_fingerprint(
                user_agent,
                accept_language,
                accept_encoding,
            )),
        }
    }

    /// The client IP address, if available.
    pub fn ip_value(&self) -> Option<&str> {
        self.ip.as_deref()
    }

    /// The client user-agent string, if available.
    pub fn user_agent_value(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }

    /// The parsed human-readable device name, if available.
    pub fn device_name_value(&self) -> Option<&str> {
        self.device_name.as_deref()
    }

    /// The parsed device type (`"desktop"`/`"mobile"`/`"tablet"`), if available.
    pub fn device_type_value(&self) -> Option<&str> {
        self.device_type.as_deref()
    }

    /// The server-computed browser fingerprint, if available.
    pub fn fingerprint_value(&self) -> Option<&str> {
        self.fingerprint.as_deref()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ClientInfo {
    type Rejection = Error;

    /// Builds [`ClientInfo`] from request extensions and headers.
    ///
    /// Reads the IP from the [`ClientIp`] extension (inserted by
    /// [`ClientIpLayer`](crate::ip::ClientIpLayer)), then derives the
    /// user-agent, device fields, and fingerprint from request headers via
    /// [`ClientInfo::from_headers`].
    ///
    /// # Errors
    ///
    /// This extractor never fails — the `Result` type is required by
    /// [`FromRequestParts`] but the implementation always returns `Ok`.
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ip = parts.extensions.get::<ClientIp>().map(|c| c.0.to_string());
        let ua = header_str(&parts.headers, "user-agent");
        let accept_lang = header_str(&parts.headers, "accept-language");
        let accept_enc = header_str(&parts.headers, "accept-encoding");
        Ok(Self::from_headers(ip, ua, accept_lang, accept_enc))
    }
}

/// Extract a header value as a string slice, returning `""` when absent or
/// non-UTF-8.
pub fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_all_none() {
        let info = ClientInfo::new();
        assert!(info.ip_value().is_none());
        assert!(info.user_agent_value().is_none());
        assert!(info.device_name_value().is_none());
        assert!(info.device_type_value().is_none());
        assert!(info.fingerprint_value().is_none());
    }

    #[test]
    fn builder_sets_fields() {
        let info = ClientInfo::new()
            .ip("1.2.3.4")
            .user_agent("Mozilla/5.0")
            .device_name("Chrome on macOS")
            .device_type("desktop")
            .fingerprint("abc123");
        assert_eq!(info.ip_value(), Some("1.2.3.4"));
        assert_eq!(info.user_agent_value(), Some("Mozilla/5.0"));
        assert_eq!(info.device_name_value(), Some("Chrome on macOS"));
        assert_eq!(info.device_type_value(), Some("desktop"));
        assert_eq!(info.fingerprint_value(), Some("abc123"));
    }

    #[test]
    fn from_headers_populates_derived_fields() {
        let info = ClientInfo::from_headers(
            Some("10.0.0.1".to_string()),
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0",
            "en-US",
            "gzip",
        );
        assert_eq!(info.ip_value(), Some("10.0.0.1"));
        assert_eq!(info.device_name_value(), Some("Chrome on macOS"));
        assert_eq!(info.device_type_value(), Some("desktop"));
        let fp = info.fingerprint_value().unwrap();
        assert_eq!(fp.len(), 64);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn extracts_from_request_parts() {
        use std::net::IpAddr;

        let mut req = http::Request::builder()
            .header("user-agent", "Mozilla/5.0 (iPhone) Safari/605")
            .header("accept-language", "en-US")
            .header("accept-encoding", "gzip")
            .body(())
            .unwrap();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        req.extensions_mut().insert(ClientIp(ip));

        let (mut parts, _) = req.into_parts();
        let info = ClientInfo::from_request_parts(&mut parts, &())
            .await
            .unwrap();

        assert_eq!(info.ip_value(), Some("10.0.0.1"));
        assert_eq!(
            info.user_agent_value(),
            Some("Mozilla/5.0 (iPhone) Safari/605")
        );
        assert_eq!(info.device_name_value(), Some("Safari on iPhone"));
        assert_eq!(info.device_type_value(), Some("mobile"));
        assert!(info.fingerprint_value().is_some());
    }

    #[tokio::test]
    async fn extracts_with_missing_fields() {
        let req = http::Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let info = ClientInfo::from_request_parts(&mut parts, &())
            .await
            .unwrap();

        assert!(info.ip_value().is_none());
        assert_eq!(info.user_agent_value(), Some(""));
        assert_eq!(info.device_name_value(), Some("Unknown on Unknown"));
        assert_eq!(info.device_type_value(), Some("desktop"));
        assert!(info.fingerprint_value().is_some());
    }
}
