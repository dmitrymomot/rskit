use std::time::Duration;

use bytes::Bytes;
use http::Uri;
use http_body_util::{BodyExt, Full};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;

use crate::error::{Error, Result};

#[allow(dead_code)]
pub(crate) struct FetchResult {
    pub data: Bytes,
    pub content_type: String,
}

/// Validate that a URL uses http or https scheme.
#[allow(dead_code)]
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

/// Fetch a file from a URL using the provided hyper client.
///
/// Streams the response body and aborts if `max_size` is exceeded.
/// Returns the body bytes and content type from the response.
/// Hard-coded 30s timeout. No redirect following.
#[allow(dead_code)]
pub(crate) async fn fetch_url(
    client: &Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    url: &str,
    max_size: Option<usize>,
) -> Result<FetchResult> {
    let uri = validate_url(url)?;

    let request = hyper::Request::builder()
        .method(hyper::Method::GET)
        .uri(&uri)
        .body(Full::new(Bytes::new()))
        .map_err(|e| Error::internal(format!("failed to build request: {e}")))?;

    let response = tokio::time::timeout(Duration::from_secs(30), client.request(request))
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
}
