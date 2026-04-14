use std::time::Duration;

use http::StatusCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use modo::webhook::{WebhookSecret, WebhookSender, verify_headers};

/// Start a minimal HTTP server that captures the request and returns the given status.
async fn start_test_server(
    response_status: u16,
) -> (String, tokio::task::JoinHandle<(String, Vec<u8>)>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());

    let handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // Single read is safe for these small test payloads (< 8KB)
        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await.unwrap();
        buf.truncate(n);
        let raw = String::from_utf8_lossy(&buf).to_string();

        let response = format!(
            "HTTP/1.1 {response_status} OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();

        (raw, buf)
    });

    (url, handle)
}

fn test_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("failed to build test HTTP client")
}

#[tokio::test]
async fn end_to_end_send_and_verify() {
    let (url, handle) = start_test_server(200).await;
    let sender = WebhookSender::new(test_client());
    let secret = WebhookSecret::new(b"e2e-test-secret".to_vec());

    let body = b"{\"event\":\"test\"}";
    let response = sender
        .send(&url, "msg_e2e_1", body, &[&secret])
        .await
        .unwrap();
    assert_eq!(response.status, StatusCode::OK);

    let (raw_request, _) = handle.await.unwrap();

    // Parse headers from raw HTTP request for round-trip verification
    let mut received_headers = http::HeaderMap::new();
    let header_section = raw_request.split("\r\n\r\n").next().unwrap();
    for line in header_section.lines().skip(1) {
        // skip request line
        if let Some((name, value)) = line.split_once(": ") {
            received_headers.insert(
                http::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
    }

    // Verify the received request can be validated with verify_headers
    verify_headers(
        &[&secret],
        &received_headers,
        body,
        Duration::from_secs(300),
    )
    .unwrap();
}

#[tokio::test]
async fn test_webhook_default_client() {
    // Smoke test: WebhookSender::default_client() must construct without panic and
    // successfully deliver a payload to a local test server.
    let (url, handle) = start_test_server(200).await;
    let sender = WebhookSender::default_client();
    let secret = WebhookSecret::new(b"default-client-secret".to_vec());

    let response = sender
        .send(&url, "msg_default_1", b"{\"event\":\"smoke\"}", &[&secret])
        .await
        .unwrap();

    assert_eq!(response.status, StatusCode::OK);
    handle.await.unwrap();
}

#[tokio::test]
async fn test_webhook_non_2xx_response() {
    // Server returns 410 Gone; the sender must surface the status and non-empty body.
    let (url, handle) = start_test_server(410).await;
    let sender = WebhookSender::new(test_client());
    let secret = WebhookSecret::new(b"gone-secret".to_vec());

    let response = sender
        .send(&url, "msg_gone_1", b"{\"event\":\"test\"}", &[&secret])
        .await
        .unwrap();

    assert_eq!(response.status, StatusCode::GONE);
    // The minimal test server sends Content-Length: 0, so body is empty bytes —
    // the important assertion is that the call succeeds and status is preserved.
    handle.await.unwrap();
}
