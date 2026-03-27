#![cfg(feature = "http-client")]

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use modo::http::Client;

// ---------------------------------------------------------------------------
// Test server helpers
// ---------------------------------------------------------------------------

/// Start a single-connection server that reads the request and sends a canned response.
/// Returns `(url, handle)` where the handle resolves with the raw request bytes.
async fn start_server(response: &'static str) -> (String, JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());

    let handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await.unwrap();
        buf.truncate(n);
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
        buf
    });

    (url, handle)
}

/// Start a multi-connection server that handles each response in order.
/// Returns `(url, handle)` where the handle resolves when all connections are served.
async fn start_multi_server(responses: Vec<&'static str>) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());
    let handle = tokio::spawn(async move {
        for response in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await.unwrap();
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();
        }
    });
    (url, handle)
}

// ---------------------------------------------------------------------------
// Canned HTTP responses
// ---------------------------------------------------------------------------

const OK_JSON: &str = concat!(
    "HTTP/1.1 200 OK\r\n",
    "Content-Type: application/json\r\n",
    "Content-Length: 14\r\n",
    "Connection: close\r\n",
    "\r\n",
    "{\"value\":\"hi\"}"
);

const OK_TEXT: &str = concat!(
    "HTTP/1.1 200 OK\r\n",
    "Content-Type: text/plain\r\n",
    "Content-Length: 11\r\n",
    "Connection: close\r\n",
    "\r\n",
    "hello world"
);

const OK_BYTES: &str = concat!(
    "HTTP/1.1 200 OK\r\n",
    "Content-Type: application/octet-stream\r\n",
    "Content-Length: 5\r\n",
    "Connection: close\r\n",
    "\r\n",
    "ABCDE"
);

const OK_EMPTY: &str = concat!(
    "HTTP/1.1 200 OK\r\n",
    "Content-Length: 0\r\n",
    "Connection: close\r\n",
    "\r\n"
);

const NOT_FOUND: &str = concat!(
    "HTTP/1.1 404 Not Found\r\n",
    "Content-Length: 0\r\n",
    "Connection: close\r\n",
    "\r\n"
);

const SERVICE_UNAVAILABLE: &str = concat!(
    "HTTP/1.1 503 Service Unavailable\r\n",
    "Content-Length: 0\r\n",
    "Connection: close\r\n",
    "\r\n"
);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// GET request with JSON body — deserialise with `.json::<T>()`.
#[tokio::test]
async fn get_json_round_trip() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    #[derive(Deserialize)]
    struct Payload {
        value: String,
    }

    let (url, _handle) = start_server(OK_JSON).await;
    let client = Client::default();
    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    let payload: Payload = resp.json().await.unwrap();
    assert_eq!(payload.value, "hi");
}

/// POST with `.json(&payload)` — server receives `Content-Type: application/json`.
#[tokio::test]
async fn post_json_body() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    #[derive(Serialize)]
    struct Body {
        name: String,
    }

    let (url, handle) = start_server(OK_EMPTY).await;
    let client = Client::default();
    let body = Body {
        name: "modo".into(),
    };
    let resp = client.post(&url).json(&body).send().await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);

    let raw = handle.await.unwrap();
    let raw_str = String::from_utf8_lossy(&raw);
    assert!(raw_str.contains("content-type: application/json"));
    assert!(raw_str.contains(r#""name":"modo""#));
}

/// Server never responds — request with short per-request timeout returns `Err`.
#[tokio::test]
async fn timeout_returns_error() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());

    // Accept but never respond.
    let _handle = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(60)).await;
    });

    // Use a client with no retries and send with a 100ms per-request timeout override.
    let client = Client::builder().max_retries(0).build();
    let result = client
        .get(&url)
        .timeout(Duration::from_millis(100))
        .send()
        .await;
    assert!(result.is_err(), "expected error, got Ok");
}

/// Connect to a port that nothing listens on — returns `Err`.
#[tokio::test]
async fn connection_refused_returns_error() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Bind a listener, get a port, then immediately drop it so nothing is listening.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let url = format!("http://127.0.0.1:{port}");
    let client = Client::default();
    let result = client.get(&url).send().await;
    assert!(result.is_err(), "expected error on refused connection");
}

/// Server returns 404 — `.error_for_status()` returns `Err`.
#[tokio::test]
async fn error_for_status_on_4xx() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (url, _handle) = start_server(NOT_FOUND).await;
    let client = Client::default();
    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::NOT_FOUND);
    let result = resp.error_for_status();
    assert!(result.is_err(), "expected Err for 404");
}

/// Server returns 200 — `.error_for_status()` returns `Ok`.
#[tokio::test]
async fn error_for_status_passes_2xx() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (url, _handle) = start_server(OK_EMPTY).await;
    let client = Client::default();
    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    let result = resp.error_for_status();
    assert!(result.is_ok(), "expected Ok for 200");
}

/// Server returns 503 on first attempt, then 200. Client with max_retries(2) gets 200.
#[tokio::test]
async fn retry_on_503() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (url, _handle) = start_multi_server(vec![SERVICE_UNAVAILABLE, OK_EMPTY]).await;

    let client = Client::builder()
        .max_retries(2)
        .retry_backoff(Duration::ZERO)
        .build();

    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
}

/// `.text()` returns body as UTF-8 string.
#[tokio::test]
async fn text_response() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (url, _handle) = start_server(OK_TEXT).await;
    let client = Client::default();
    let resp = client.get(&url).send().await.unwrap();
    let text = resp.text().await.unwrap();
    assert_eq!(text, "hello world");
}

/// `.bytes()` returns raw bytes.
#[tokio::test]
async fn bytes_response() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (url, _handle) = start_server(OK_BYTES).await;
    let client = Client::default();
    let resp = client.get(&url).send().await.unwrap();
    let bytes = resp.bytes().await.unwrap();
    assert_eq!(&bytes[..], b"ABCDE");
}

/// `.stream().next()` yields body data.
#[tokio::test]
async fn stream_yields_chunks() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (url, _handle) = start_server(OK_TEXT).await;
    let client = Client::default();
    let resp = client.get(&url).send().await.unwrap();
    let mut stream = resp.stream();

    let mut collected = Vec::new();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.unwrap();
        collected.extend_from_slice(&bytes);
    }

    assert_eq!(collected, b"hello world");
}

/// `client.get("not a url").send().await` returns `Err`.
#[tokio::test]
async fn invalid_url_returns_error() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let client = Client::default();
    let result = client.get("not a url").send().await;
    assert!(result.is_err(), "expected Err for invalid URL");
}

/// Server receives `Authorization: Bearer test-token`.
#[tokio::test]
async fn bearer_token_header() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (url, handle) = start_server(OK_EMPTY).await;
    let client = Client::default();
    client
        .get(&url)
        .bearer_token("test-token")
        .send()
        .await
        .unwrap();

    let raw = handle.await.unwrap();
    let raw_str = String::from_utf8_lossy(&raw);
    assert!(
        raw_str.contains("authorization: Bearer test-token"),
        "authorization header not found in: {raw_str}"
    );
}

/// Server receives URL with `?foo=bar&baz=qux`.
#[tokio::test]
async fn query_params_appended() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (url, handle) = start_server(OK_EMPTY).await;
    let client = Client::default();
    client
        .get(&url)
        .query(&[("foo", "bar"), ("baz", "qux")])
        .send()
        .await
        .unwrap();

    let raw = handle.await.unwrap();
    let raw_str = String::from_utf8_lossy(&raw);
    // The request line should include the query string.
    assert!(
        raw_str.contains("?foo=bar&baz=qux"),
        "query params not found in: {raw_str}"
    );
}

/// `response.content_length()` returns the value from the `Content-Length` header.
#[tokio::test]
async fn content_length_from_headers() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (url, _handle) = start_server(OK_TEXT).await;
    let client = Client::default();
    let resp = client.get(&url).send().await.unwrap();
    let cl = resp.content_length();
    assert_eq!(
        cl,
        Some(11),
        "content_length should be 11 for 'hello world'"
    );
}

/// `bearer_token` with invalid characters (newlines) returns a deferred error at `send()`.
#[tokio::test]
async fn bearer_token_invalid_chars_returns_error() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let client = Client::default();
    let result = client
        .get("http://127.0.0.1:1")
        .bearer_token("token\nwith\nnewlines")
        .send()
        .await;
    assert!(result.is_err(), "expected error for invalid bearer token");
}

/// Client has 30s default timeout; per-request override of 100ms fires quickly.
#[tokio::test]
async fn per_request_timeout_override() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());

    // Accept but never respond.
    let _handle = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(60)).await;
    });

    let client = Client::builder().timeout(Duration::from_secs(30)).build();

    let start = std::time::Instant::now();
    let result = client
        .get(&url)
        .timeout(Duration::from_millis(100))
        .send()
        .await;
    let elapsed = start.elapsed();

    assert!(result.is_err(), "expected timeout error");
    // Should have timed out well before the 30s default.
    assert!(
        elapsed < Duration::from_secs(5),
        "per-request timeout did not fire in time: {elapsed:?}"
    );
}
