use axum::extract::FromRequestParts;
use http::request::Parts;

use crate::error::Error;
use crate::ip::ClientIp;

/// Client request context: IP address, user-agent, and fingerprint.
///
/// Implements [`FromRequestParts`] for automatic extraction in handlers.
/// Requires [`ClientIpLayer`](crate::ip::ClientIpLayer) for the `ip` field;
/// if the layer is absent, `ip` will be `None`.
///
/// For non-HTTP contexts (background jobs, CLI tools), use the builder:
///
/// ```
/// use modo::ip::ClientInfo;
///
/// let info = ClientInfo::new()
///     .ip("1.2.3.4")
///     .user_agent("my-script/1.0");
/// ```
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
    ip: Option<String>,
    user_agent: Option<String>,
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

    /// Set the client fingerprint (from the `x-fingerprint` header).
    pub fn fingerprint(mut self, fp: impl Into<String>) -> Self {
        self.fingerprint = Some(fp.into());
        self
    }

    /// The client IP address, if available.
    pub fn ip_value(&self) -> Option<&str> {
        self.ip.as_deref()
    }

    /// The client user-agent string, if available.
    pub fn user_agent_value(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }

    /// The client fingerprint, if available.
    pub fn fingerprint_value(&self) -> Option<&str> {
        self.fingerprint.as_deref()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ClientInfo {
    type Rejection = Error;

    /// Builds [`ClientInfo`] from request extensions and headers.
    ///
    /// Reads the IP from the [`ClientIp`] extension (inserted by
    /// [`ClientIpLayer`](crate::ip::ClientIpLayer)), the `User-Agent` header,
    /// and the `X-Fingerprint` header. Any field that cannot be read is `None`.
    ///
    /// # Errors
    ///
    /// This extractor never fails — the `Result` type is required by
    /// [`FromRequestParts`] but the implementation always returns `Ok`.
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ip = parts.extensions.get::<ClientIp>().map(|c| c.0.to_string());

        let user_agent = parts
            .headers
            .get(http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let fingerprint = parts
            .headers
            .get("x-fingerprint")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        Ok(Self {
            ip,
            user_agent,
            fingerprint,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_all_none() {
        let info = ClientInfo::new();
        assert!(info.ip_value().is_none());
        assert!(info.user_agent_value().is_none());
        assert!(info.fingerprint_value().is_none());
    }

    #[test]
    fn builder_sets_fields() {
        let info = ClientInfo::new()
            .ip("1.2.3.4")
            .user_agent("Mozilla/5.0")
            .fingerprint("abc123");
        assert_eq!(info.ip_value(), Some("1.2.3.4"));
        assert_eq!(info.user_agent_value(), Some("Mozilla/5.0"));
        assert_eq!(info.fingerprint_value(), Some("abc123"));
    }

    #[tokio::test]
    async fn extracts_from_request_parts() {
        use crate::ip::ClientIp;
        use std::net::IpAddr;

        let mut req = http::Request::builder()
            .header("user-agent", "TestAgent/1.0")
            .header("x-fingerprint", "fp_abc")
            .body(())
            .unwrap();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        req.extensions_mut().insert(ClientIp(ip));

        let (mut parts, _) = req.into_parts();
        let info = ClientInfo::from_request_parts(&mut parts, &())
            .await
            .unwrap();

        assert_eq!(info.ip_value(), Some("10.0.0.1"));
        assert_eq!(info.user_agent_value(), Some("TestAgent/1.0"));
        assert_eq!(info.fingerprint_value(), Some("fp_abc"));
    }

    #[tokio::test]
    async fn extracts_with_missing_fields() {
        let req = http::Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let info = ClientInfo::from_request_parts(&mut parts, &())
            .await
            .unwrap();

        assert!(info.ip_value().is_none());
        assert!(info.user_agent_value().is_none());
        assert!(info.fingerprint_value().is_none());
    }
}
