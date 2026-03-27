use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

/// An HTTP response received from an outgoing request.
///
/// Provides typed accessors for status, headers, and URL, plus terminal methods
/// to consume the body as JSON, UTF-8 text, raw bytes, or a streaming iterator.
///
/// # Examples
///
/// ```rust,ignore
/// let resp = client.get("https://api.example.com/data").send().await?;
/// let data: MyStruct = resp.error_for_status()?.json().await?;
/// ```
pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    url: String,
    body: hyper::body::Incoming,
}

impl Response {
    /// Create a new `Response` from its constituent parts.
    pub(crate) fn new(
        status: StatusCode,
        headers: HeaderMap,
        url: String,
        body: hyper::body::Incoming,
    ) -> Self {
        Self {
            status,
            headers,
            url,
            body,
        }
    }

    /// Returns the HTTP status code.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the response headers.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Returns the URL that produced this response.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the `Content-Length` header value, if present and valid.
    pub fn content_length(&self) -> Option<u64> {
        self.headers
            .get(http::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
    }

    /// Consume the body and deserialize it as JSON.
    pub async fn json<T: DeserializeOwned>(self) -> Result<T> {
        let bytes = self.bytes().await?;
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::internal(format!("failed to parse response JSON: {e}")).chain(e))
    }

    /// Consume the body and return it as a UTF-8 string.
    pub async fn text(self) -> Result<String> {
        let bytes = self.bytes().await?;
        String::from_utf8(bytes.into())
            .map_err(|e| Error::internal("response body is not valid UTF-8").chain(e))
    }

    /// Consume the body and return it as raw bytes.
    pub async fn bytes(self) -> Result<Bytes> {
        self.body
            .collect()
            .await
            .map(|collected| collected.to_bytes())
            .map_err(|e| Error::internal(format!("failed to read response body: {e}")).chain(e))
    }

    /// Convert this response into a streaming body reader.
    pub fn stream(self) -> BodyStream {
        BodyStream { body: self.body }
    }

    /// Return `Ok(self)` if the status is 2xx, otherwise return an error.
    pub fn error_for_status(self) -> Result<Self> {
        if self.status.is_success() {
            Ok(self)
        } else {
            Err(Error::internal(format!(
                "HTTP {}: {}",
                self.status, self.url
            )))
        }
    }
}

/// A streaming reader over a response body.
///
/// Each call to [`next`](BodyStream::next) yields the next data frame as raw bytes,
/// skipping trailers and other non-data frames. Returns `None` when the body is
/// exhausted.
pub struct BodyStream {
    body: hyper::body::Incoming,
}

impl BodyStream {
    /// Read the next data frame from the body stream.
    ///
    /// Returns `None` when the stream is complete.
    pub async fn next(&mut self) -> Option<Result<Bytes>> {
        loop {
            let frame = std::pin::pin!(self.body.frame()).await?;
            match frame {
                Ok(frame) => {
                    if let Ok(data) = frame.into_data() {
                        return Some(Ok(data));
                    }
                    // Non-data frame (e.g. trailers) — skip and read next.
                }
                Err(e) => {
                    return Some(Err(Error::internal(format!(
                        "failed to read response body: {e}"
                    ))
                    .chain(e)));
                }
            }
        }
    }
}
