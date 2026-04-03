use bytes::Bytes;
use http::{HeaderMap, StatusCode};

use crate::error::{Error, Result};

/// Response returned after a webhook delivery attempt.
pub struct WebhookResponse {
    /// HTTP status code returned by the endpoint.
    pub status: StatusCode,
    /// Response body bytes.
    pub body: Bytes,
}

/// Send a webhook POST via the shared HTTP client.
pub(crate) async fn post(
    client: &reqwest::Client,
    url: &str,
    headers: HeaderMap,
    body: Bytes,
) -> Result<WebhookResponse> {
    let response = client
        .post(url)
        .headers(headers)
        .body(body)
        .send()
        .await
        .map_err(|e| Error::internal("webhook delivery failed").chain(e))?;
    let status = response.status();
    let response_body = response
        .bytes()
        .await
        .map_err(|e| Error::internal("failed to read webhook response").chain(e))?;

    Ok(WebhookResponse {
        status,
        body: response_body,
    })
}
