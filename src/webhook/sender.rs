use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::HeaderMap;

use super::client::{HttpClient, HyperClient, WebhookResponse};
use super::secret::WebhookSecret;
use super::signature::sign_headers;
use crate::error::{Error, Result};

struct WebhookSenderInner<C: HttpClient> {
    client: C,
    user_agent: String,
}

/// High-level webhook sender that signs and delivers payloads using the
/// Standard Webhooks protocol.
///
/// Clone-cheap: the inner state is wrapped in `Arc`.
pub struct WebhookSender<C: HttpClient> {
    inner: Arc<WebhookSenderInner<C>>,
}

impl<C: HttpClient> Clone for WebhookSender<C> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<C: HttpClient> WebhookSender<C> {
    /// Create a new sender with the given HTTP client.
    pub fn new(client: C) -> Self {
        Self {
            inner: Arc::new(WebhookSenderInner {
                client,
                user_agent: format!("modo-webhooks/{}", env!("CARGO_PKG_VERSION")),
            }),
        }
    }

    /// Override the default user-agent string.
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        let inner =
            Arc::get_mut(&mut self.inner).expect("with_user_agent must be called before cloning");
        inner.user_agent = user_agent.into();
        self
    }

    /// Send a webhook following the Standard Webhooks protocol.
    ///
    /// - `url`: the endpoint to POST to
    /// - `id`: unique message ID for idempotency (e.g. `msg_<ulid>`)
    /// - `body`: raw request body (typically JSON)
    /// - `secrets`: one or more signing secrets (supports key rotation)
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
        headers.insert("user-agent", self.inner.user_agent.parse().unwrap());
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

        self.inner
            .client
            .post(url, headers, Bytes::copy_from_slice(body))
            .await
    }
}

impl WebhookSender<HyperClient> {
    /// Convenience constructor with default HyperClient (30s timeout).
    pub fn default_client() -> Self {
        Self::new(HyperClient::new(Duration::from_secs(30)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    struct MockClient {
        response_status: StatusCode,
        captured_headers: std::sync::Mutex<Option<HeaderMap>>,
        captured_body: std::sync::Mutex<Option<Bytes>>,
    }

    impl MockClient {
        fn new(status: StatusCode) -> Self {
            Self {
                response_status: status,
                captured_headers: std::sync::Mutex::new(None),
                captured_body: std::sync::Mutex::new(None),
            }
        }

        fn captured_headers(&self) -> HeaderMap {
            self.captured_headers.lock().unwrap().clone().unwrap()
        }

        fn captured_body(&self) -> Bytes {
            self.captured_body.lock().unwrap().clone().unwrap()
        }
    }

    impl HttpClient for MockClient {
        async fn post(
            &self,
            _url: &str,
            headers: HeaderMap,
            body: Bytes,
        ) -> Result<WebhookResponse> {
            *self.captured_headers.lock().unwrap() = Some(headers);
            *self.captured_body.lock().unwrap() = Some(body);
            Ok(WebhookResponse {
                status: self.response_status,
                body: Bytes::new(),
            })
        }
    }

    #[tokio::test]
    async fn send_sets_correct_headers() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock);
        let secret = WebhookSecret::new(b"test-key".to_vec());

        let result = sender
            .send("http://example.com/hook", "msg_123", b"{}", &[&secret])
            .await;
        assert!(result.is_ok());

        let headers = sender.inner.client.captured_headers();
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
        assert_eq!(headers.get("webhook-id").unwrap(), "msg_123");
        assert!(headers.get("webhook-timestamp").is_some());
        assert!(
            headers
                .get("webhook-signature")
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("v1,")
        );
    }

    #[tokio::test]
    async fn send_default_user_agent() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock);
        let secret = WebhookSecret::new(b"key".to_vec());

        sender
            .send("http://example.com/hook", "msg_1", b"{}", &[&secret])
            .await
            .unwrap();

        let headers = sender.inner.client.captured_headers();
        let ua = headers.get("user-agent").unwrap().to_str().unwrap();
        assert!(ua.starts_with("modo-webhooks/"));
    }

    #[tokio::test]
    async fn send_custom_user_agent() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock).with_user_agent("my-app/2.0");
        let secret = WebhookSecret::new(b"key".to_vec());

        sender
            .send("http://example.com/hook", "msg_1", b"{}", &[&secret])
            .await
            .unwrap();

        let headers = sender.inner.client.captured_headers();
        assert_eq!(headers.get("user-agent").unwrap(), "my-app/2.0");
    }

    #[tokio::test]
    async fn send_empty_secrets_rejected() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock);

        let result = sender
            .send("http://example.com/hook", "msg_1", b"{}", &[])
            .await;
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("secret"));
    }

    #[tokio::test]
    async fn send_empty_id_rejected() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock);
        let secret = WebhookSecret::new(b"key".to_vec());

        let result = sender
            .send("http://example.com/hook", "", b"{}", &[&secret])
            .await;
        assert!(result.is_err());
        assert!(result.err().unwrap().message().contains("id"));
    }

    #[tokio::test]
    async fn send_empty_body_accepted() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock);
        let secret = WebhookSecret::new(b"key".to_vec());

        let result = sender
            .send("http://example.com/hook", "msg_1", b"", &[&secret])
            .await;
        assert!(result.is_ok());
        assert!(sender.inner.client.captured_body().is_empty());
    }

    #[tokio::test]
    async fn send_returns_response_status() {
        let mock = MockClient::new(StatusCode::GONE);
        let sender = WebhookSender::new(mock);
        let secret = WebhookSecret::new(b"key".to_vec());

        let response = sender
            .send("http://example.com/hook", "msg_1", b"{}", &[&secret])
            .await
            .unwrap();
        assert_eq!(response.status, StatusCode::GONE);
    }

    #[tokio::test]
    async fn send_invalid_url_rejected() {
        let mock = MockClient::new(StatusCode::OK);
        let sender = WebhookSender::new(mock);
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
