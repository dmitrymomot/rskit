use http::{HeaderMap, StatusCode};
use serde::de::DeserializeOwned;

/// The captured response from an in-process request sent via [`super::TestRequestBuilder`].
///
/// Provides convenience accessors for status code, headers, and body in text,
/// JSON, or raw-bytes form.
pub struct TestResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl TestResponse {
    /// Construct a `TestResponse` from its raw parts.
    ///
    /// This is called internally by [`super::TestRequestBuilder::send`]; you
    /// rarely need to construct one manually outside of unit tests for this
    /// type itself.
    pub fn new(status: StatusCode, headers: HeaderMap, body: Vec<u8>) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    /// Return the HTTP status code as a `u16`.
    pub fn status(&self) -> u16 {
        self.status.as_u16()
    }

    /// Return the first value of the response header `name`, or `None` if the
    /// header is absent or its value is not valid UTF-8.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    /// Return all values of the response header `name`.
    ///
    /// Useful for headers that may appear multiple times, such as `set-cookie`.
    /// Values that are not valid UTF-8 are silently omitted.
    pub fn header_all(&self, name: &str) -> Vec<&str> {
        self.headers
            .get_all(name)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect()
    }

    /// Interpret the response body as a UTF-8 string.
    ///
    /// # Panics
    ///
    /// Panics if the body is not valid UTF-8.
    pub fn text(&self) -> &str {
        std::str::from_utf8(&self.body).expect("response body is not valid UTF-8")
    }

    /// Deserialize the response body as JSON into `T`.
    ///
    /// # Panics
    ///
    /// Panics if deserialization fails.
    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).expect("failed to deserialize response body as JSON")
    }

    /// Return the raw response body bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }
}
