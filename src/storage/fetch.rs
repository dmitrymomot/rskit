use std::time::Duration;

use bytes::Bytes;
use http::Uri;
use http_body_util::{BodyExt, Full};

use crate::error::{Error, Result};

pub(crate) struct FetchResult {
    pub data: Bytes,
    pub content_type: String,
}

/// Validate that a URL uses http or https scheme.
fn validate_url(url: &str) -> Result<Uri> {
    let uri: Uri = url
        .parse()
        .map_err(|e| Error::bad_request(format!("invalid URL: {e}")))?;
    match uri.scheme_str() {
        Some("http") | Some("https") => Ok(uri),
        Some(scheme) => Err(Error::bad_request(format!(
            "URL must use http or https scheme, got {scheme}"
        ))),
        None => Err(Error::bad_request("URL must use http or https scheme")),
    }
}

/// Fetch a file from a URL using the provided HTTP client.
///
/// Streams the response body and aborts if `max_size` is exceeded.
/// Returns the body bytes and content type from the response.
/// Hard-coded 30s timeout. No redirect following.
pub(crate) async fn fetch_url(
    client: &crate::http::Client,
    url: &str,
    max_size: Option<usize>,
) -> Result<FetchResult> {
    let uri = validate_url(url)?;

    let request = hyper::Request::builder()
        .method(hyper::Method::GET)
        .uri(&uri)
        .body(Full::new(Bytes::new()))
        .map_err(|e| Error::internal(format!("failed to build request: {e}")))?;

    let response =
        tokio::time::timeout(Duration::from_secs(30), client.raw_client().request(request))
        .await
        .map_err(|_| Error::internal("URL fetch timed out"))?
        .map_err(|e| Error::internal(format!("failed to fetch URL: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(Error::bad_request(format!(
            "failed to fetch URL ({status})"
        )));
    }

    let content_type = response
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let mut body = response.into_body();
    let mut buf: Vec<u8> = Vec::new();

    loop {
        let frame = match std::pin::pin!(body.frame()).await {
            Some(Ok(frame)) => frame,
            Some(Err(e)) => {
                return Err(Error::internal(format!(
                    "failed to read response body: {e}"
                )));
            }
            None => break,
        };

        if let Some(chunk) = frame.data_ref() {
            buf.extend_from_slice(chunk);
            if let Some(max) = max_size
                && buf.len() > max
            {
                return Err(Error::payload_too_large(format!(
                    "fetched file size exceeds maximum {max}"
                )));
            }
        }
    }

    Ok(FetchResult {
        data: Bytes::from(buf),
        content_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn validate_url_accepts_https() {
        assert!(validate_url("https://example.com/file.jpg").is_ok());
    }

    #[test]
    fn validate_url_accepts_http() {
        assert!(validate_url("http://example.com/file.jpg").is_ok());
    }

    #[test]
    fn validate_url_rejects_ftp() {
        let err = validate_url("ftp://example.com/file.jpg").err().unwrap();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_url_rejects_no_scheme() {
        let err = validate_url("example.com/file.jpg").err().unwrap();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_url_rejects_empty() {
        assert!(validate_url("").is_err());
    }

    #[test]
    fn validate_url_rejects_garbage() {
        assert!(validate_url("not a url at all").is_err());
    }

    fn build_test_client() -> crate::http::Client {
        let _ = rustls::crypto::ring::default_provider().install_default();
        crate::http::Client::default()
    }

    async fn start_server(
        body: &'static [u8],
        content_type: Option<&str>,
        status: u16,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://127.0.0.1:{}", addr.port());

        let ct_header = match content_type {
            Some(ct) => format!("Content-Type: {ct}\r\n"),
            None => String::new(),
        };

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await.unwrap();

            let response = format!(
                "HTTP/1.1 {status} OK\r\n{ct_header}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.write_all(body).await.unwrap();
            stream.shutdown().await.unwrap();
        });

        (url, handle)
    }

    #[tokio::test]
    async fn fetch_url_success_with_content_type() {
        let (url, handle) = start_server(b"image data", Some("image/png"), 200).await;
        let client = build_test_client();

        let result = fetch_url(&client, &url, None).await.unwrap();
        assert_eq!(result.data, Bytes::from_static(b"image data"));
        assert_eq!(result.content_type, "image/png");

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn fetch_url_fallback_content_type() {
        let (url, handle) = start_server(b"binary data", None, 200).await;
        let client = build_test_client();

        let result = fetch_url(&client, &url, None).await.unwrap();
        assert_eq!(result.content_type, "application/octet-stream");

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn fetch_url_rejects_non_2xx() {
        let (url, handle) = start_server(b"not found", Some("text/plain"), 404).await;
        let client = build_test_client();

        let err = fetch_url(&client, &url, None).await.err().unwrap();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn fetch_url_enforces_max_size() {
        let big_body: &[u8] = b"this body exceeds the limit";
        let (url, handle) = start_server(big_body, Some("text/plain"), 200).await;
        let client = build_test_client();

        let err = fetch_url(&client, &url, Some(5)).await.err().unwrap();
        assert_eq!(err.status(), http::StatusCode::PAYLOAD_TOO_LARGE);

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn fetch_url_redirect_returns_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://127.0.0.1:{}", addr.port());

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await.unwrap();

            let response = "HTTP/1.1 301 Moved Permanently\r\nLocation: http://example.com/new\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();
        });

        let client = build_test_client();
        let err = fetch_url(&client, &url, None).await.err().unwrap();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn fetch_url_content_type_preserved_from_response() {
        let (url, handle) = start_server(b"pdf content", Some("application/pdf"), 200).await;
        let client = build_test_client();

        let result = fetch_url(&client, &url, None).await.unwrap();
        assert_eq!(result.content_type, "application/pdf");

        handle.await.unwrap();
    }
}
