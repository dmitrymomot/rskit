use http::Uri;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::de::DeserializeOwned;

pub(crate) async fn post_form<T: DeserializeOwned>(
    url: &str,
    params: &[(&str, &str)],
) -> crate::Result<T> {
    let body = serde_urlencoded::to_string(params)
        .map_err(|e| crate::Error::internal(format!("failed to encode form: {e}")))?;

    let uri: Uri = url
        .parse()
        .map_err(|e| crate::Error::internal(format!("invalid URL: {e}")))?;

    let connector = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_only()
        .enable_http1()
        .build();
    let client = Client::builder(TokioExecutor::new()).build(connector);

    let request = hyper::Request::builder()
        .method(hyper::Method::POST)
        .uri(uri)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .body(Full::new(Bytes::from(body)))
        .map_err(|e| crate::Error::internal(format!("failed to build request: {e}")))?;

    let response = client
        .request(request)
        .await
        .map_err(|e| crate::Error::internal(format!("HTTP request failed: {e}")))?;

    let status = response.status();
    let body_bytes = response
        .into_body()
        .collect()
        .await
        .map_err(|e| crate::Error::internal(format!("failed to read response body: {e}")))?
        .to_bytes();

    if !status.is_success() {
        let body_str = String::from_utf8_lossy(&body_bytes);
        return Err(crate::Error::internal(format!(
            "OAuth token exchange failed ({status}): {body_str}"
        )));
    }

    serde_json::from_slice(&body_bytes)
        .map_err(|e| crate::Error::internal(format!("failed to parse response JSON: {e}")))
}

pub(crate) async fn get_json<T: DeserializeOwned>(url: &str, token: &str) -> crate::Result<T> {
    let uri: Uri = url
        .parse()
        .map_err(|e| crate::Error::internal(format!("invalid URL: {e}")))?;

    let connector = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_only()
        .enable_http1()
        .build();
    let client = Client::builder(TokioExecutor::new()).build(connector);

    let request = hyper::Request::builder()
        .method(hyper::Method::GET)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("accept", "application/json")
        .header("user-agent", "modo/0.1")
        .body(Full::new(Bytes::new()))
        .map_err(|e| crate::Error::internal(format!("failed to build request: {e}")))?;

    let response = client
        .request(request)
        .await
        .map_err(|e| crate::Error::internal(format!("HTTP request failed: {e}")))?;

    let status = response.status();
    let body_bytes = response
        .into_body()
        .collect()
        .await
        .map_err(|e| crate::Error::internal(format!("failed to read response body: {e}")))?
        .to_bytes();

    if !status.is_success() {
        let body_str = String::from_utf8_lossy(&body_bytes);
        return Err(crate::Error::internal(format!(
            "OAuth API request failed ({status}): {body_str}"
        )));
    }

    serde_json::from_slice(&body_bytes)
        .map_err(|e| crate::Error::internal(format!("failed to parse response JSON: {e}")))
}
