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
    client
        .post(url)
        .form(&params)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}

pub(crate) async fn get_json<T: DeserializeOwned>(
    client: &crate::http::Client,
    url: &str,
    token: &str,
) -> crate::Result<T> {
    client
        .get(url)
        .bearer_token(token)
        .header(
            http::header::ACCEPT,
            http::header::HeaderValue::from_static("application/json"),
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}
