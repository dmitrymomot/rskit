use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::time::Duration;

use tokio::net::UdpSocket;

use crate::error::{Error, Result};

use super::error::DnsError;
use super::protocol::{self, RecordType};

/// Internal trait for DNS resolution. Object-safe via `Pin<Box<dyn Future>>`.
/// Not public — exists for test mocking.
pub(crate) trait DnsResolver: Send + Sync {
    fn resolve_txt(
        &self,
        domain: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>>;

    fn resolve_cname(
        &self,
        domain: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>>;
}

/// UDP-based DNS resolver. Sends queries to a single nameserver.
pub(crate) struct UdpDnsResolver {
    pub(crate) nameserver: SocketAddr,
    pub(crate) timeout: Duration,
}

impl UdpDnsResolver {
    pub(crate) fn new(nameserver: SocketAddr, timeout: Duration) -> Self {
        Self {
            nameserver,
            timeout,
        }
    }

    async fn send_and_receive(&self, query_bytes: &[u8]) -> Result<Vec<u8>> {
        let socket = UdpSocket::bind("0.0.0.0:0").await.map_err(|_| {
            Error::bad_gateway("dns network error")
                .chain(DnsError::NetworkError)
                .with_code(DnsError::NetworkError.code())
        })?;

        socket
            .send_to(query_bytes, self.nameserver)
            .await
            .map_err(|_| {
                Error::bad_gateway("dns network error")
                    .chain(DnsError::NetworkError)
                    .with_code(DnsError::NetworkError.code())
            })?;

        let mut buf = [0u8; 512];
        let len = tokio::time::timeout(self.timeout, socket.recv(&mut buf))
            .await
            .map_err(|_| {
                Error::gateway_timeout("dns query timed out")
                    .chain(DnsError::Timeout)
                    .with_code(DnsError::Timeout.code())
            })?
            .map_err(|_| {
                Error::bad_gateway("dns network error")
                    .chain(DnsError::NetworkError)
                    .with_code(DnsError::NetworkError.code())
            })?;

        Ok(buf[..len].to_vec())
    }
}

impl DnsResolver for UdpDnsResolver {
    fn resolve_txt(
        &self,
        domain: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + '_>> {
        let domain = domain.to_owned();
        Box::pin(async move {
            let (query_id, query_bytes) = protocol::build_query(&domain, RecordType::Txt)?;
            let response_bytes = self.send_and_receive(&query_bytes).await?;
            let packet = protocol::validate_response(&response_bytes, query_id)?;
            Ok(protocol::extract_txt_records(&packet))
        })
    }

    fn resolve_cname(
        &self,
        domain: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>> {
        let domain = domain.to_owned();
        Box::pin(async move {
            let (query_id, query_bytes) = protocol::build_query(&domain, RecordType::Cname)?;
            let response_bytes = self.send_and_receive(&query_bytes).await?;
            let packet = protocol::validate_response(&response_bytes, query_id)?;
            Ok(protocol::extract_cname_target(&packet))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udp_resolver_stores_config() {
        let addr: SocketAddr = "8.8.8.8:53".parse().unwrap();
        let timeout = Duration::from_millis(3000);
        let resolver = UdpDnsResolver::new(addr, timeout);
        assert_eq!(resolver.nameserver, addr);
        assert_eq!(resolver.timeout, timeout);
    }
}
