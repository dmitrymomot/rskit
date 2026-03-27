use bytes::Bytes;
use http::{HeaderMap, StatusCode};

use crate::error::Result;

/// Response returned after a webhook delivery attempt.
pub struct WebhookResponse {
    /// HTTP status code returned by the endpoint.
    pub status: StatusCode,
    /// Response body bytes.
    pub body: Bytes,
}

/// Send a webhook POST via the shared HTTP client.
pub(crate) async fn post(
    client: &crate::http::Client,
    url: &str,
    headers: HeaderMap,
    body: Bytes,
) -> Result<WebhookResponse> {
    let response = client.post(url).headers(headers).body(body).send().await?;
    let status = response.status();
    let response_body = response.bytes().await?;

    Ok(WebhookResponse {
        status,
        body: response_body,
    })
}
