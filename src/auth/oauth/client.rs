//! Internal HTTP helpers used by provider implementations.

use serde::de::DeserializeOwned;

pub(crate) async fn post_form<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    params: &[(&str, &str)],
) -> crate::Result<T> {
    let resp = client
        .post(url)
        .header(http::header::ACCEPT, "application/json")
        .form(&params)
        .send()
        .await
        .map_err(|e| {
            crate::Error::internal(format!("OAuth token exchange failed: {e}")).chain(e)
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(crate::Error::internal(format!(
            "OAuth token exchange failed ({status}): {body}"
        )));
    }

    resp.json()
        .await
        .map_err(|e| crate::Error::internal("failed to parse OAuth token response").chain(e))
}

pub(crate) async fn get_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> crate::Result<T> {
    let resp = client
        .get(url)
        .bearer_auth(token)
        .header(http::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| crate::Error::internal(format!("OAuth API request failed: {e}")).chain(e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(crate::Error::internal(format!(
            "OAuth API request failed ({status}): {body}"
        )));
    }

    resp.json()
        .await
        .map_err(|e| crate::Error::internal("failed to parse OAuth API response").chain(e))
}
