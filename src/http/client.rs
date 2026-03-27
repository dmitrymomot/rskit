use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::Full;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy;
use hyper_util::rt::TokioExecutor;

use super::config::ClientConfig;
use super::request::RequestBuilder;

/// Type alias for the hyper HTTPS client used internally.
pub(crate) type InnerHttpsClient = legacy::Client<
    hyper_rustls::HttpsConnector<legacy::connect::HttpConnector>,
    Full<Bytes>,
>;

/// Shared inner state for [`Client`].
pub(crate) struct ClientInner {
    pub(crate) client: InnerHttpsClient,
    pub(crate) config: ClientConfig,
}

/// A reusable HTTP client with connection pooling, timeouts, and retry support.
///
/// `Client` is cheap to clone — it wraps its state in an `Arc`. All clones
/// share the same connection pool and configuration.
///
/// # Examples
///
/// ```rust,ignore
/// use modo::http::{Client, ClientConfig};
///
/// let client = Client::new(&ClientConfig::default());
/// let resp = client.get("https://api.example.com/data").send().await?;
/// let body = resp.text().await?;
/// ```
#[derive(Clone)]
pub struct Client {
    inner: Arc<ClientInner>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new(&ClientConfig::default())
    }
}

impl Client {
    /// Create a new client from the given configuration.
    pub fn new(config: &ClientConfig) -> Self {
        let mut http_connector = legacy::connect::HttpConnector::new();
        http_connector.set_connect_timeout(Some(config.connect_timeout()));

        let connector = HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .wrap_connector(http_connector);

        let client = legacy::Client::builder(TokioExecutor::new()).build(connector);

        Self {
            inner: Arc::new(ClientInner {
                client,
                config: config.clone(),
            }),
        }
    }

    /// Start building a client with a [`ClientBuilder`].
    pub fn builder() -> ClientBuilder {
        ClientBuilder {
            config: ClientConfig::default(),
        }
    }

    /// Start a GET request to the given URL.
    pub fn get(&self, url: &str) -> RequestBuilder {
        self.request(http::Method::GET, url)
    }

    /// Start a POST request to the given URL.
    pub fn post(&self, url: &str) -> RequestBuilder {
        self.request(http::Method::POST, url)
    }

    /// Start a PUT request to the given URL.
    pub fn put(&self, url: &str) -> RequestBuilder {
        self.request(http::Method::PUT, url)
    }

    /// Start a PATCH request to the given URL.
    pub fn patch(&self, url: &str) -> RequestBuilder {
        self.request(http::Method::PATCH, url)
    }

    /// Start a DELETE request to the given URL.
    pub fn delete(&self, url: &str) -> RequestBuilder {
        self.request(http::Method::DELETE, url)
    }

    /// Start a request with an arbitrary HTTP method and URL.
    pub fn request(&self, method: http::Method, url: &str) -> RequestBuilder {
        RequestBuilder::new(self.inner.clone(), method, url)
    }

    /// Access the underlying hyper client for pre-signed request dispatch.
    ///
    /// Used internally by the storage module for AWS Signature V4 requests
    /// that need full control over request construction.
    #[cfg_attr(not(feature = "storage"), allow(dead_code))]
    pub(crate) fn raw_client(&self) -> &InnerHttpsClient {
        &self.inner.client
    }
}

/// A fluent builder for constructing a [`Client`].
///
/// # Examples
///
/// ```rust,ignore
/// use modo::http::Client;
///
/// let client = Client::builder()
///     .timeout(Duration::from_secs(10))
///     .max_retries(3)
///     .build();
/// ```
pub struct ClientBuilder {
    config: ClientConfig,
}

impl ClientBuilder {
    /// Set the default request timeout.
    pub fn timeout(mut self, d: Duration) -> Self {
        self.config.timeout_secs = d.as_secs();
        self
    }

    /// Set the TCP connect timeout.
    pub fn connect_timeout(mut self, d: Duration) -> Self {
        self.config.connect_timeout_secs = d.as_secs();
        self
    }

    /// Set the `User-Agent` header value.
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.config.user_agent = ua.into();
        self
    }

    /// Set the maximum number of retry attempts for retryable failures.
    pub fn max_retries(mut self, n: u32) -> Self {
        self.config.max_retries = n;
        self
    }

    /// Set the initial retry backoff duration.
    pub fn retry_backoff(mut self, d: Duration) -> Self {
        self.config.retry_backoff_ms = d.as_millis() as u64;
        self
    }

    /// Build the client.
    pub fn build(self) -> Client {
        Client::new(&self.config)
    }
}
