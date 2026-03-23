use http::{HeaderMap, StatusCode};
use serde::de::DeserializeOwned;

pub struct TestResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl TestResponse {
    pub fn new(status: StatusCode, headers: HeaderMap, body: Vec<u8>) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    pub fn status(&self) -> u16 {
        self.status.as_u16()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    pub fn header_all(&self, name: &str) -> Vec<&str> {
        self.headers
            .get_all(name)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect()
    }

    pub fn text(&self) -> &str {
        std::str::from_utf8(&self.body).expect("response body is not valid UTF-8")
    }

    pub fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).expect("failed to deserialize response body as JSON")
    }

    pub fn bytes(&self) -> &[u8] {
        &self.body
    }
}
