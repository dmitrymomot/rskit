use std::sync::Arc;

use bytes::Bytes;
use http::HeaderMap;

use super::client::{self, WebhookResponse};
use super::secret::WebhookSecret;
use super::signature::sign_headers;
use crate::error::{Error, Result};

struct WebhookSenderInner {
    client: crate::http::Client,
    user_agent: String,
}

/// High-level webhook sender that signs and delivers payloads using the
/// Standard Webhooks protocol.
///
/// Clone-cheap: the inner state is wrapped in `Arc`.
pub struct WebhookSender {
    inner: Arc<WebhookSenderInner>,
}

impl Clone for WebhookSender {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl WebhookSender {
    /// Create a new sender with the given HTTP client.
    pub fn new(client: crate::http::Client) -> Self {
        Self {
            inner: Arc::new(WebhookSenderInner {
                client,
                user_agent: format!("modo-webhooks/{}", env!("CARGO_PKG_VERSION")),
            }),
        }
    }

    /// Override the default `User-Agent` header sent with every request.
    ///
    /// The value must be a valid HTTP header value (visible ASCII only, no
    /// control characters). Invalid values are silently ignored.
    ///
    /// # Panics
    ///
    /// Panics if called after the sender has been cloned. Call this immediately
    /// after [`WebhookSender::new`] before handing clones to other tasks.
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        let ua = user_agent.into();
        // Validate before storing — prevents panic in send().
        if http::header::HeaderValue::from_str(&ua).is_err() {
            return self;
        }
        let inner =
            Arc::get_mut(&mut self.inner).expect("with_user_agent must be called before cloning");
        inner.user_agent = ua;
        self
    }

    /// Convenience constructor using a default [`crate::http::Client`] with a
    /// 30-second timeout.
    pub fn default_client() -> Self {
        Self::new(
            crate::http::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build(),
        )
    }

    /// Send a webhook following the Standard Webhooks protocol.
    ///
    /// Signs the payload with every secret in `secrets` (supports key rotation)
    /// and POSTs to `url` with the three Standard Webhooks headers:
    /// `webhook-id`, `webhook-timestamp`, and `webhook-signature`.
    ///
    /// - `url`: the endpoint to POST to
    /// - `id`: unique message ID for idempotency (e.g. `msg_<ulid>`)
    /// - `body`: raw request body (typically JSON)
    /// - `secrets`: one or more signing secrets; at least one is required
    pub async fn send(
        &self,
        url: &str,
        id: &str,
        body: &[u8],
        secrets: &[&WebhookSecret],
    ) -> Result<WebhookResponse> {
        if secrets.is_empty() {
            return Err(Error::bad_request("at least one secret required"));
        }
        if id.is_empty() {
            return Err(Error::bad_request("webhook id must not be empty"));
        }
        // Validate URL early — it comes from user/app input
        let _: http::Uri = url
            .parse()
            .map_err(|e| Error::bad_request(format!("invalid webhook url: {e}")))?;

        let timestamp = chrono::Utc::now().timestamp();
        let signed = sign_headers(secrets, id, timestamp, body);

        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert(
            "user-agent",
            self.inner
                .user_agent
                .parse()
                .map_err(|_| Error::internal("invalid user-agent header value"))?,
        );
        headers.insert(
            "webhook-id",
            signed
                .webhook_id
                .parse()
                .map_err(|_| Error::bad_request("webhook id contains invalid header characters"))?,
        );
        headers.insert(
            "webhook-timestamp",
            signed.webhook_timestamp.to_string().parse().unwrap(),
        );
        headers.insert(
            "webhook-signature",
            signed
                .webhook_signature
                .parse()
                .map_err(|_| Error::internal("generated invalid webhook-signature header"))?,
        );

        client::post(
            &self.inner.client,
            url,
            headers,
            Bytes::copy_from_slice(body),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use http::StatusCode;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    /// Start a minimal HTTP server that captures the request and returns the given status.
    async fn start_test_server(response_status: u16) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://127.0.0.1:{}", addr.port());

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).await.unwrap();
            buf.truncate(n);
            let raw = String::from_utf8_lossy(&buf).to_string();

            let response = format!(
                "HTTP/1.1 {response_status} OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();

            raw
        });

        (url, handle)
    }

    fn test_client() -> crate::http::Client {
        crate::http::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
    }

    #[tokio::test]
    async fn send_sets_correct_headers() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (url, handle) = start_test_server(200).await;

        let sender = WebhookSender::new(test_client());
        let secret = WebhookSecret::new(b"test-key".to_vec());

        let result = sender.send(&url, "msg_123", b"{}", &[&secret]).await;
        assert!(result.is_ok());

        let raw = handle.await.unwrap();
        assert!(raw.contains("content-type: application/json"));
        assert!(raw.contains("webhook-id: msg_123"));
        assert!(raw.contains("webhook-timestamp:"));
        assert!(raw.contains("webhook-signature: v1,"));
    }

    #[tokio::test]
    async fn send_default_user_agent() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (url, handle) = start_test_server(200).await;

        let sender = WebhookSender::new(test_client());
        let secret = WebhookSecret::new(b"key".to_vec());

        sender.send(&url, "msg_1", b"{}", &[&secret]).await.unwrap();

        let raw = handle.await.unwrap();
        assert!(raw.contains("user-agent: modo-webhooks/"));
    }

    #[tokio::test]
    async fn send_custom_user_agent() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (url, handle) = start_test_server(200).await;

        let sender = WebhookSender::new(test_client()).with_user_agent("my-app/2.0");
        let secret = WebhookSecret::new(b"key".to_vec());

        sender.send(&url, "msg_1", b"{}", &[&secret]).await.unwrap();

        let raw = handle.await.unwrap();
        assert!(raw.contains("user-agent: my-app/2.0"));
    }

    #[tokio::test]
    async fn send_empty_secrets_rejected() {
        let sender = WebhookSender::new(test_client());

        let result = sender
            .send("http://example.com/hook", "msg_1", b"{}", &[])
            .await;
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("secret"));
    }

    #[tokio::test]
    async fn send_empty_id_rejected() {
        let sender = WebhookSender::new(test_client());
        let secret = WebhookSecret::new(b"key".to_vec());

        let result = sender
            .send("http://example.com/hook", "", b"{}", &[&secret])
            .await;
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("id"));
    }

    #[tokio::test]
    async fn send_empty_body_accepted() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (url, handle) = start_test_server(200).await;

        let sender = WebhookSender::new(test_client());
        let secret = WebhookSecret::new(b"key".to_vec());

        let result = sender.send(&url, "msg_1", b"", &[&secret]).await;
        assert!(result.is_ok());

        let raw = handle.await.unwrap();
        // The request was sent — verify it reached the server
        assert!(raw.contains("POST / HTTP/1.1"));
    }

    #[tokio::test]
    async fn send_returns_response_status() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (url, handle) = start_test_server(410).await;

        let sender = WebhookSender::new(test_client());
        let secret = WebhookSecret::new(b"key".to_vec());

        let response = sender.send(&url, "msg_1", b"{}", &[&secret]).await.unwrap();
        assert_eq!(response.status, StatusCode::GONE);

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn send_invalid_url_rejected() {
        let sender = WebhookSender::new(test_client());
        let secret = WebhookSecret::new(b"key".to_vec());

        let result = sender
            .send("not a valid url", "msg_1", b"{}", &[&secret])
            .await;
        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .message()
                .contains("invalid webhook url")
        );
    }
}
