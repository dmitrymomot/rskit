use axum::body::Body;
use http::{Method, Request};
use serde::Serialize;
use tower::ServiceExt;

use super::response::TestResponse;

pub struct TestRequestBuilder {
    router: axum::Router,
    method: Method,
    uri: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}

impl TestRequestBuilder {
    pub fn new(router: axum::Router, method: Method, uri: &str) -> Self {
        Self {
            router,
            method,
            uri: uri.to_string(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    pub fn json<T: Serialize>(mut self, body: &T) -> Self {
        let bytes = serde_json::to_vec(body).expect("failed to serialize JSON body");
        self.headers
            .push(("content-type".to_string(), "application/json".to_string()));
        self.body = Some(bytes);
        self
    }

    pub fn form<T: Serialize>(mut self, body: &T) -> Self {
        let encoded = serde_urlencoded::to_string(body).expect("failed to serialize form body");
        self.headers.push((
            "content-type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        ));
        self.body = Some(encoded.into_bytes());
        self
    }

    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub async fn send(self) -> TestResponse {
        let body = match self.body {
            Some(bytes) => Body::from(bytes),
            None => Body::empty(),
        };

        let mut request = Request::builder().method(self.method).uri(self.uri);
        for (key, value) in &self.headers {
            request = request.header(key.as_str(), value.as_str());
        }
        let request = request.body(body).expect("failed to build request");

        let response = self.router.oneshot(request).await.expect("request failed");

        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("failed to read response body");

        TestResponse::new(status, headers, body.to_vec())
    }
}
