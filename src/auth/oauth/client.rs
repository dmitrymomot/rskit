//! Internal HTTP helpers used by provider implementations.
//!
//! Functions take a shared `http::Client` reference, gaining connection pooling
//! via the framework-wide HTTP client.

use serde::de::DeserializeOwned;

pub(crate) async fn post_form<T: DeserializeOwned>(
    client: &crate::http::Client,
    url: &str,
    params: &[(&str, &str)],
) -> crate::Result<T> {
    let resp = client
        .post(url)
        .header(
            http::header::ACCEPT,
            http::header::HeaderValue::from_static("application/json"),
        )
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(crate::Error::internal(format!(
            "OAuth token exchange failed ({status}): {body}"
        )));
    }

    resp.json().await
}

pub(crate) async fn get_json<T: DeserializeOwned>(
    client: &crate::http::Client,
    url: &str,
    token: &str,
) -> crate::Result<T> {
    let resp = client
        .get(url)
        .bearer_token(token)
        .header(
            http::header::ACCEPT,
            http::header::HeaderValue::from_static("application/json"),
        )
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(crate::Error::internal(format!(
            "OAuth API request failed ({status}): {body}"
        )));
    }

    resp.json().await
}
