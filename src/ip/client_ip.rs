use std::net::IpAddr;

use axum::extract::FromRequestParts;
use http::request::Parts;

use crate::error::Error;

/// Resolved client IP address.
///
/// Inserted into request extensions by [`ClientIpLayer`](super::ClientIpLayer).
/// Use as an axum extractor in handlers:
///
/// ```
/// use modo::ClientIp;
///
/// async fn handler(ClientIp(ip): ClientIp) -> String {
///     ip.to_string()
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub IpAddr);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<ClientIp>().copied().ok_or_else(|| {
            Error::internal("ClientIp not found in request extensions — is ClientIpLayer applied?")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::request::Parts;

    fn parts_with_client_ip(ip: IpAddr) -> Parts {
        let mut req = http::Request::builder().body(()).unwrap();
        req.extensions_mut().insert(ClientIp(ip));
        req.into_parts().0
    }

    fn parts_without_client_ip() -> Parts {
        let req = http::Request::builder().body(()).unwrap();
        req.into_parts().0
    }

    #[tokio::test]
    async fn extracts_client_ip_from_extensions() {
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let mut parts = parts_with_client_ip(ip);
        let result = ClientIp::from_request_parts(&mut parts, &()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, ip);
    }

    #[tokio::test]
    async fn returns_error_when_missing() {
        let mut parts = parts_without_client_ip();
        let result = ClientIp::from_request_parts(&mut parts, &()).await;
        assert!(result.is_err());
    }
}
