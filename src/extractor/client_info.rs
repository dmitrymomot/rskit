use axum::extract::FromRequestParts;
use http::request::Parts;

use crate::error::Error;
use crate::ip::ClientIp;

/// Client request context: IP address, user-agent, and fingerprint.
///
/// Implements [`FromRequestParts`] for automatic extraction in handlers.
/// Requires [`ClientIpLayer`](crate::ClientIpLayer) for the `ip` field;
/// if the layer is absent, `ip` will be `None`.
///
/// For non-HTTP contexts (background jobs, CLI tools), use the builder:
///
/// ```
/// use modo::extractor::ClientInfo;
///
/// let info = ClientInfo::new()
///     .ip("1.2.3.4")
///     .user_agent("my-script/1.0");
/// ```
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub fingerprint: Option<String>,
}

impl ClientInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ip(mut self, ip: impl Into<String>) -> Self {
        self.ip = Some(ip.into());
        self
    }

    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    pub fn fingerprint(mut self, fp: impl Into<String>) -> Self {
        self.fingerprint = Some(fp.into());
        self
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ClientInfo {
    type Rejection = Error;

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
        assert!(info.ip.is_none());
        assert!(info.user_agent.is_none());
        assert!(info.fingerprint.is_none());
    }

    #[test]
    fn builder_sets_fields() {
        let info = ClientInfo::new()
            .ip("1.2.3.4")
            .user_agent("Mozilla/5.0")
            .fingerprint("abc123");
        assert_eq!(info.ip.as_deref(), Some("1.2.3.4"));
        assert_eq!(info.user_agent.as_deref(), Some("Mozilla/5.0"));
        assert_eq!(info.fingerprint.as_deref(), Some("abc123"));
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
        let info = ClientInfo::from_request_parts(&mut parts, &()).await.unwrap();

        assert_eq!(info.ip.as_deref(), Some("10.0.0.1"));
        assert_eq!(info.user_agent.as_deref(), Some("TestAgent/1.0"));
        assert_eq!(info.fingerprint.as_deref(), Some("fp_abc"));
    }

    #[tokio::test]
    async fn extracts_with_missing_fields() {
        let req = http::Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let info = ClientInfo::from_request_parts(&mut parts, &()).await.unwrap();

        assert!(info.ip.is_none());
        assert!(info.user_agent.is_none());
        assert!(info.fingerprint.is_none());
    }
}
