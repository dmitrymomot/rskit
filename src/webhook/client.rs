use std::future::Future;
use std::time::Duration;

use bytes::Bytes;
use http::HeaderMap;
use http::StatusCode;
use http_body_util::{BodyExt, Full};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use crate::error::{Error, Result};

/// Response from a webhook delivery attempt.
pub struct WebhookResponse {
    pub status: StatusCode,
    pub body: Bytes,
}

/// Trait for sending webhook HTTP POST requests.
/// RPITIT — not object-safe, used as concrete type parameter.
pub trait HttpClient: Send + Sync + 'static {
    fn post(
        &self,
        url: &str,
        headers: HeaderMap,
        body: Bytes,
    ) -> impl Future<Output = Result<WebhookResponse>> + Send;
}

/// Default hyper-based HTTP client with TLS support.
pub struct HyperClient {
    client: Client<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
        Full<Bytes>,
    >,
    timeout: Duration,
}

impl HyperClient {
    pub fn new(timeout: Duration) -> Self {
        let connector = HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .build();
        let client = Client::builder(TokioExecutor::new()).build(connector);
        Self { client, timeout }
    }
}

impl HttpClient for HyperClient {
    async fn post(&self, url: &str, headers: HeaderMap, body: Bytes) -> Result<WebhookResponse> {
        let uri: http::Uri = url
            .parse()
            .map_err(|e| Error::bad_request(format!("invalid webhook url: {e}")))?;

        let mut builder = hyper::Request::builder()
            .method(hyper::Method::POST)
            .uri(uri);

        for (name, value) in &headers {
            builder = builder.header(name, value);
        }

        let request = builder
            .body(Full::new(body))
            .map_err(|e| Error::internal(format!("failed to build webhook request: {e}")))?;

        let response = tokio::time::timeout(self.timeout, self.client.request(request))
            .await
            .map_err(|_| Error::internal("webhook request timed out"))?
            .map_err(|e| Error::internal(format!("webhook request failed: {e}")))?;

        let status = response.status();
        let response_body = response
            .into_body()
            .collect()
            .await
            .map_err(|e| Error::internal(format!("failed to read webhook response: {e}")))?
            .to_bytes();

        Ok(WebhookResponse {
            status,
            body: response_body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyper_client_creates_without_panic() {
        let _ = HyperClient::new(Duration::from_secs(30));
    }
}
