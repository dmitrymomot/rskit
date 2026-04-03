# Reqwest Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the custom hyper-based HTTP client module (`src/http/`, 1048 lines) with direct `reqwest::Client` usage in all consumer modules.

**Architecture:** Delete `src/http/` entirely. Each consumer module (`embed`, `auth`, `storage`, `webhooks`) uses `reqwest::Client` directly — no wrapper. The `http-client` feature flag is removed; each consumer feature activates `dep:reqwest` independently.

**Tech Stack:** reqwest 0.13 (rustls-tls, json, stream features)

---

### Task 1: Update Cargo.toml — add reqwest, remove old deps, update features

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Replace dependency block and feature flags**

In `Cargo.toml`, make these changes:

1. Remove the `http-client` feature entirely (line 27).
2. Update consumer features to use `dep:reqwest` instead of `http-client`:

```toml
auth = [
  "dep:reqwest",
  "dep:argon2",
  "dep:hmac",
  "dep:sha1",
]
```

```toml
storage = ["dep:reqwest", "dep:hmac"]
```

```toml
webhooks = ["dep:reqwest", "dep:hmac", "dep:base64"]
```

```toml
text-embedding = ["dep:reqwest"]
```

3. In the `full` feature, remove `http-client` (`text-embedding` is already listed and now pulls `dep:reqwest` directly):

```toml
full = ["db", "session", "job", "templates", "sse", "auth", "sentry", "email", "storage", "webhooks", "dns", "geolocation", "qrcode", "apikey", "text-embedding", "tier"]
```

4. Remove these optional dependencies:
   - `hyper` (line 104)
   - `hyper-rustls` (lines 105-111)
   - `hyper-util` (line 112)
   - `http-body-util` (line 113)

5. Add `reqwest` in the dependencies section (where hyper deps were):

```toml
reqwest = { version = "0.13", optional = true, default-features = false, features = [
  "rustls-tls",
  "json",
  "stream",
] }
```

6. Note: `base64` stays as an optional dep (line 114) — it's now activated by `webhooks` instead of `http-client`.

- [ ] **Step 2: Verify toml syntax**

Run: `cargo check --no-default-features 2>&1 | head -5`

This should parse Cargo.toml without errors. Features won't compile yet (consumer code still references old types), but the manifest itself must be valid.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: replace hyper deps with reqwest, update feature flags"
```

---

### Task 2: Delete custom HTTP module and all references from lib.rs/config

**Files:**
- Delete: `src/http/mod.rs`, `src/http/client.rs`, `src/http/config.rs`, `src/http/request.rs`, `src/http/response.rs`, `src/http/retry.rs`, `src/http/README.md`
- Delete: `tests/http_client.rs`
- Modify: `src/lib.rs`
- Modify: `src/config/modo.rs`

- [ ] **Step 1: Delete the src/http/ directory and test file**

```bash
rm -rf src/http
rm tests/http_client.rs
```

- [ ] **Step 2: Remove HTTP module declaration and re-exports from lib.rs**

In `src/lib.rs`, remove these lines:

```rust
#[cfg(feature = "http-client")]
pub mod http;
```

And remove these re-exports:

```rust
#[cfg(feature = "http-client")]
pub use http::{
    Client as HttpClient, ClientBuilder as HttpClientBuilder, ClientConfig as HttpClientConfig,
};
```

Also update the doc comment (line 17) — remove `http-client` from the feature list:

Change:
```rust
/// feature flags: `session`, `job`, `http-client`, `auth`, `templates`,
```
To:
```rust
/// feature flags: `session`, `job`, `auth`, `templates`,
```

- [ ] **Step 3: Remove HTTP config from modo config struct**

In `src/config/modo.rs`, remove these lines (39-43):

```rust
    /// HTTP client settings (timeout, retries, user agent).
    /// Requires the `http-client` feature.
    #[cfg(feature = "http-client")]
    #[serde(default)]
    pub http: crate::http::ClientConfig,
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: delete custom HTTP client module and references"
```

---

### Task 3: Migrate embed module to reqwest

**Files:**
- Modify: `src/embed/openai.rs`
- Modify: `src/embed/gemini.rs`
- Modify: `src/embed/mistral.rs`
- Modify: `src/embed/voyage.rs`

All four files follow the exact same pattern. The changes are:
1. Replace `use crate::http;` → remove (no import needed, use `reqwest::Client` fully qualified)
2. Replace `http::Client` → `reqwest::Client` in Inner struct and constructor
3. Replace `.bearer_token(...)` → `.bearer_auth(...)`
4. Add `.map_err(...)` to `.send().await` and `.json().await` and `.text().await`

- [ ] **Step 1: Migrate openai.rs**

In `src/embed/openai.rs`:

Replace `use crate::http;` (line 7) → remove the line entirely.

Replace `client: http::Client,` (line 16) → `client: reqwest::Client,`

Replace constructor parameter (line 47):
```rust
    pub fn new(client: http::Client, config: &OpenAIConfig) -> Result<Self> {
```
→
```rust
    pub fn new(client: reqwest::Client, config: &OpenAIConfig) -> Result<Self> {
```

Replace the request block (lines 76-83):
```rust
            let resp = self
                .0
                .client
                .post(&url)
                .bearer_token(&self.0.api_key)
                .json(&body)
                .send()
                .await?;
```
→
```rust
            let resp = self
                .0
                .client
                .post(&url)
                .bearer_auth(&self.0.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| Error::internal(format!("openai request failed: {e}")).chain(e))?;
```

Replace `.text().await` (line 87):
```rust
                let text = resp.text().await.unwrap_or_default();
```
→
```rust
                let text = resp.text().await.unwrap_or_default();
```
(No change needed — reqwest's `.text().await` returns `Result<String>` and `unwrap_or_default()` handles the error case.)

Replace `.json().await` (lines 93-95):
```rust
            let parsed: Response = resp.json().await.map_err(|e| {
                Error::internal("failed to parse openai embedding response").chain(e)
            })?;
```
→
```rust
            let parsed: Response = resp.json().await.map_err(|e| {
                Error::internal("failed to parse openai embedding response").chain(e)
            })?;
```
(No change needed — reqwest's `.json::<T>()` returns `Result<T, reqwest::Error>` which `.chain()` accepts.)

- [ ] **Step 2: Migrate gemini.rs**

In `src/embed/gemini.rs`:

Remove `use crate::http;` (line 7).

Replace `client: http::Client,` (line 16) → `client: reqwest::Client,`

Replace constructor parameter (line 46):
```rust
    pub fn new(client: http::Client, config: &GeminiConfig) -> Result<Self> {
```
→
```rust
    pub fn new(client: reqwest::Client, config: &GeminiConfig) -> Result<Self> {
```

Replace the request block (lines 69-80):
```rust
            let resp = self
                .0
                .client
                .post(&url)
                .header(
                    ::http::header::HeaderName::from_static("x-goog-api-key"),
                    ::http::header::HeaderValue::from_str(&self.0.api_key)
                        .map_err(|e| Error::internal("invalid gemini api key header").chain(e))?,
                )
                .json(&body)
                .send()
                .await?;
```
→
```rust
            let resp = self
                .0
                .client
                .post(&url)
                .header("x-goog-api-key", &self.0.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| Error::internal(format!("gemini request failed: {e}")).chain(e))?;
```

Replace `.json().await` (lines 90-92) — same pattern, no change needed.

- [ ] **Step 3: Migrate mistral.rs**

In `src/embed/mistral.rs`:

Remove `use crate::http;` (line 7).

Replace `client: http::Client,` (line 19) → `client: reqwest::Client,`

Replace constructor parameter (line 49):
```rust
    pub fn new(client: http::Client, config: &MistralConfig) -> Result<Self> {
```
→
```rust
    pub fn new(client: reqwest::Client, config: &MistralConfig) -> Result<Self> {
```

Replace the request block (lines 69-76):
```rust
            let resp = self
                .0
                .client
                .post(URL)
                .bearer_token(&self.0.api_key)
                .json(&body)
                .send()
                .await?;
```
→
```rust
            let resp = self
                .0
                .client
                .post(URL)
                .bearer_auth(&self.0.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| Error::internal(format!("mistral request failed: {e}")).chain(e))?;
```

- [ ] **Step 4: Migrate voyage.rs**

In `src/embed/voyage.rs`:

Remove `use crate::http;` (line 7).

Replace `client: http::Client,` (line 14) → `client: reqwest::Client,`

Replace constructor parameter (line 45):
```rust
    pub fn new(client: http::Client, config: &VoyageConfig) -> Result<Self> {
```
→
```rust
    pub fn new(client: reqwest::Client, config: &VoyageConfig) -> Result<Self> {
```

Replace the request block (lines 67-74):
```rust
            let resp = self
                .0
                .client
                .post(URL)
                .bearer_token(&self.0.api_key)
                .json(&body)
                .send()
                .await?;
```
→
```rust
            let resp = self
                .0
                .client
                .post(URL)
                .bearer_auth(&self.0.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| Error::internal(format!("voyage request failed: {e}")).chain(e))?;
```

- [ ] **Step 5: Commit**

```bash
git add src/embed/openai.rs src/embed/gemini.rs src/embed/mistral.rs src/embed/voyage.rs
git commit -m "refactor: migrate embed module to reqwest"
```

---

### Task 4: Migrate auth/oauth module to reqwest

**Files:**
- Modify: `src/auth/oauth/client.rs`
- Modify: `src/auth/oauth/google.rs`
- Modify: `src/auth/oauth/github.rs`

- [ ] **Step 1: Rewrite oauth/client.rs helpers**

Replace the entire content of `src/auth/oauth/client.rs` with:

```rust
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
        .map_err(|e| crate::Error::internal(format!("OAuth token exchange failed: {e}")).chain(e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(crate::Error::internal(format!(
            "OAuth token exchange failed ({status}): {body}"
        )));
    }

    resp.json().await.map_err(|e| {
        crate::Error::internal("failed to parse OAuth token response").chain(e)
    })
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

    resp.json().await.map_err(|e| {
        crate::Error::internal("failed to parse OAuth API response").chain(e)
    })
}
```

- [ ] **Step 2: Update google.rs**

In `src/auth/oauth/google.rs`:

Replace the struct field (line 29):
```rust
    http_client: crate::http::Client,
```
→
```rust
    http_client: reqwest::Client,
```

Replace the constructor parameter (line 41):
```rust
        http_client: crate::http::Client,
```
→
```rust
        http_client: reqwest::Client,
```

- [ ] **Step 3: Update github.rs**

In `src/auth/oauth/github.rs`:

Replace the struct field (line 34):
```rust
    http_client: crate::http::Client,
```
→
```rust
    http_client: reqwest::Client,
```

Replace the constructor parameter (line 46):
```rust
        http_client: crate::http::Client,
```
→
```rust
        http_client: reqwest::Client,
```

- [ ] **Step 4: Commit**

```bash
git add src/auth/oauth/client.rs src/auth/oauth/google.rs src/auth/oauth/github.rs
git commit -m "refactor: migrate auth/oauth module to reqwest"
```

---

### Task 5: Migrate webhook module to reqwest

**Files:**
- Modify: `src/webhook/client.rs`
- Modify: `src/webhook/sender.rs`

- [ ] **Step 1: Rewrite webhook/client.rs**

Replace the content of `src/webhook/client.rs` with:

```rust
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
        .map_err(|e| Error::internal(format!("webhook delivery failed: {e}")).chain(e))?;
    let status = response.status();
    let response_body = response
        .bytes()
        .await
        .map_err(|e| Error::internal(format!("failed to read webhook response: {e}")).chain(e))?;

    Ok(WebhookResponse {
        status,
        body: response_body,
    })
}
```

- [ ] **Step 2: Update webhook/sender.rs**

In `src/webhook/sender.rs`, replace the struct field (line 12):
```rust
    client: crate::http::Client,
```
→
```rust
    client: reqwest::Client,
```

Replace the constructor (line 34):
```rust
    pub fn new(client: crate::http::Client) -> Self {
```
→
```rust
    pub fn new(client: reqwest::Client) -> Self {
```

Replace `default_client()` (lines 66-72):
```rust
    pub fn default_client() -> Self {
        Self::new(
            crate::http::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build(),
        )
    }
```
→
```rust
    pub fn default_client() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build default webhook HTTP client");
        Self::new(client)
    }
```

Replace the `test_client()` helper in the `#[cfg(test)]` block (lines 186-190):
```rust
    fn test_client() -> crate::http::Client {
        crate::http::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
    }
```
→
```rust
    fn test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("failed to build test HTTP client")
    }
```

Remove the line `let _ = rustls::crypto::ring::default_provider().install_default();` from every test function that has it (lines 194, 213, 226, 263, 279). reqwest handles its own TLS provider initialization.

- [ ] **Step 3: Commit**

```bash
git add src/webhook/client.rs src/webhook/sender.rs
git commit -m "refactor: migrate webhook module to reqwest"
```

---

### Task 6: Migrate storage module to reqwest

This is the most complex task — the storage module uses raw hyper APIs for AWS Signature V4 requests.

**Files:**
- Modify: `src/storage/client.rs`
- Modify: `src/storage/backend.rs`
- Modify: `src/storage/facade.rs`
- Modify: `src/storage/fetch.rs`

- [ ] **Step 1: Rewrite storage/client.rs**

The entire file needs rewriting. Replace `src/storage/client.rs` contents with:

```rust
use std::time::Duration;

use bytes::Bytes;

use super::options::PutOptions;
use super::presign::{PresignParams, presign_url};
use super::signing::{SigningParams, sign_request, uri_encode};
use crate::error::{Error, Result};

pub(crate) struct RemoteBackend {
    client: reqwest::Client,
    bucket: String,
    endpoint: String,
    endpoint_host: String,
    access_key: String,
    secret_key: String,
    region: String,
    path_style: bool,
}

/// SHA-256 hash of an empty body (used for DELETE, HEAD, GET).
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

impl RemoteBackend {
    pub fn new(
        client: reqwest::Client,
        bucket: String,
        endpoint: String,
        access_key: String,
        secret_key: String,
        region: String,
        path_style: bool,
    ) -> Result<Self> {
        let endpoint_host = strip_scheme(&endpoint).to_string();

        Ok(Self {
            client,
            bucket,
            endpoint,
            endpoint_host,
            access_key,
            secret_key,
            region,
            path_style,
        })
    }

    pub async fn put(
        &self,
        key: &str,
        data: Bytes,
        content_type: &str,
        opts: &PutOptions,
    ) -> Result<()> {
        let (url, host) = self.url_and_host(key);
        let canonical_uri = self.canonical_uri(key);

        let mut extra_headers = vec![("content-type".to_string(), content_type.to_string())];
        if let Some(ref cd) = opts.content_disposition {
            extra_headers.push(("content-disposition".to_string(), cd.clone()));
        }
        if let Some(ref cc) = opts.cache_control {
            extra_headers.push(("cache-control".to_string(), cc.clone()));
        }
        if let Some(acl) = &opts.acl {
            extra_headers.push(("x-amz-acl".to_string(), acl.as_header_value().to_string()));
        }

        let params = SigningParams {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            method: "PUT",
            canonical_uri: &canonical_uri,
            host: &host,
            query_string: "",
            extra_headers: &extra_headers,
            payload_hash: "UNSIGNED-PAYLOAD",
            now: chrono::Utc::now(),
        };
        let (auth, signed_headers) = sign_request(&params);

        let content_length = data.len();
        let mut req = self.client.put(&url);
        for (k, v) in &signed_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        req = req
            .header("authorization", &auth)
            .header("content-length", content_length.to_string());

        let response = req
            .body(data)
            .send()
            .await
            .map_err(|e| Error::internal(format!("PUT request failed: {e}")).chain(e))?;

        let status = response.status();
        if !status.is_success() {
            let body_str = response.text().await.unwrap_or_default();
            return Err(Error::internal(format!(
                "PUT failed ({status}): {body_str}"
            )));
        }

        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        let (url, host) = self.url_and_host(key);
        let canonical_uri = self.canonical_uri(key);

        let params = SigningParams {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            method: "DELETE",
            canonical_uri: &canonical_uri,
            host: &host,
            query_string: "",
            extra_headers: &[],
            payload_hash: EMPTY_SHA256,
            now: chrono::Utc::now(),
        };
        let (auth, signed_headers) = sign_request(&params);

        let mut req = self.client.delete(&url);
        for (k, v) in &signed_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        req = req.header("authorization", &auth);

        let response = req
            .send()
            .await
            .map_err(|e| Error::internal(format!("DELETE request failed: {e}")).chain(e))?;

        let status = response.status();
        if !status.is_success() {
            let body_str = response.text().await.unwrap_or_default();
            return Err(Error::internal(format!(
                "DELETE failed ({status}): {body_str}"
            )));
        }

        Ok(())
    }

    pub async fn exists(&self, key: &str) -> Result<bool> {
        let (url, host) = self.url_and_host(key);
        let canonical_uri = self.canonical_uri(key);

        let params = SigningParams {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            method: "HEAD",
            canonical_uri: &canonical_uri,
            host: &host,
            query_string: "",
            extra_headers: &[],
            payload_hash: EMPTY_SHA256,
            now: chrono::Utc::now(),
        };
        let (auth, signed_headers) = sign_request(&params);

        let mut req = self.client.head(&url);
        for (k, v) in &signed_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        req = req.header("authorization", &auth);

        let response = req
            .send()
            .await
            .map_err(|e| Error::internal(format!("HEAD request failed: {e}")).chain(e))?;

        match response.status() {
            s if s.is_success() => Ok(true),
            http::StatusCode::NOT_FOUND => Ok(false),
            status => Err(Error::internal(format!("HEAD failed ({status})"))),
        }
    }

    pub async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let mut all_keys = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut query = format!("list-type=2&prefix={}", uri_encode(prefix, true));
            if let Some(ref token) = continuation_token {
                query.push_str(&format!("&continuation-token={}", uri_encode(token, true)));
            }

            // List is always at bucket root
            let (base_url, host) = if self.path_style {
                (
                    format!("{}/{}?{}", self.endpoint, self.bucket, query),
                    self.endpoint_host.clone(),
                )
            } else {
                (
                    format!("https://{}.{}/?{}", self.bucket, self.endpoint_host, query),
                    format!("{}.{}", self.bucket, self.endpoint_host),
                )
            };
            let canonical_uri = if self.path_style {
                format!("/{}", self.bucket)
            } else {
                "/".to_string()
            };

            let params = SigningParams {
                access_key: &self.access_key,
                secret_key: &self.secret_key,
                region: &self.region,
                method: "GET",
                canonical_uri: &canonical_uri,
                host: &host,
                query_string: &query,
                extra_headers: &[],
                payload_hash: EMPTY_SHA256,
                now: chrono::Utc::now(),
            };
            let (auth, signed_headers) = sign_request(&params);

            let mut req = self.client.get(&base_url);
            for (k, v) in &signed_headers {
                req = req.header(k.as_str(), v.as_str());
            }
            req = req.header("authorization", &auth);

            let response = req
                .send()
                .await
                .map_err(|e| Error::internal(format!("LIST request failed: {e}")).chain(e))?;

            let status = response.status();
            let body = response
                .bytes()
                .await
                .map_err(|e| Error::internal(format!("failed to read response: {e}")).chain(e))?;

            if !status.is_success() {
                let body_str = String::from_utf8_lossy(&body);
                return Err(Error::internal(format!(
                    "LIST failed ({status}): {body_str}"
                )));
            }

            let body_str = String::from_utf8_lossy(&body);

            // Hand-parse <Key>...</Key> values
            for key in extract_xml_values(&body_str, "Key") {
                all_keys.push(key);
            }

            // Check pagination
            let is_truncated = extract_xml_value(&body_str, "IsTruncated")
                .map(|v| v == "true")
                .unwrap_or(false);

            if is_truncated {
                continuation_token = extract_xml_value(&body_str, "NextContinuationToken");
            } else {
                break;
            }
        }

        Ok(all_keys)
    }

    pub async fn presigned_url(&self, key: &str, expires_in: Duration) -> Result<String> {
        let params = PresignParams {
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            bucket: &self.bucket,
            key,
            endpoint: &self.endpoint,
            endpoint_host: &self.endpoint_host,
            path_style: self.path_style,
            expires_in,
            now: chrono::Utc::now(),
        };
        Ok(presign_url(&params))
    }

    fn url_and_host(&self, key: &str) -> (String, String) {
        build_url_and_host(
            &self.endpoint,
            &self.endpoint_host,
            &self.bucket,
            key,
            self.path_style,
        )
    }

    fn canonical_uri(&self, key: &str) -> String {
        build_canonical_uri(&self.bucket, key, self.path_style)
    }
}

// Free functions exposed for unit tests

fn build_url_and_host(
    endpoint: &str,
    endpoint_host: &str,
    bucket: &str,
    key: &str,
    path_style: bool,
) -> (String, String) {
    let encoded_key = uri_encode(key, false);
    if path_style {
        (
            format!("{endpoint}/{bucket}/{encoded_key}"),
            endpoint_host.to_string(),
        )
    } else {
        (
            format!("https://{bucket}.{endpoint_host}/{encoded_key}"),
            format!("{bucket}.{endpoint_host}"),
        )
    }
}

fn build_canonical_uri(bucket: &str, key: &str, path_style: bool) -> String {
    let encoded_key = uri_encode(key, false);
    if path_style {
        format!("/{bucket}/{encoded_key}")
    } else {
        format!("/{encoded_key}")
    }
}

fn strip_scheme(endpoint: &str) -> &str {
    endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"))
        .unwrap_or(endpoint)
}

/// Extract all values between `<tag>` and `</tag>` from XML.
fn extract_xml_values(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut values = Vec::new();
    let mut search_from = 0;
    while let Some(start) = xml[search_from..].find(&open) {
        let abs_start = search_from + start + open.len();
        if let Some(end) = xml[abs_start..].find(&close) {
            values.push(xml[abs_start..abs_start + end].to_string());
            search_from = abs_start + end + close.len();
        } else {
            break;
        }
    }
    values
}

/// Extract a single value between `<tag>` and `</tag>`.
fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    extract_xml_values(xml, tag).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_path_style() {
        let (url, _) = build_url_and_host(
            "https://s3.example.com",
            "s3.example.com",
            "mybucket",
            "photos/cat.jpg",
            true,
        );
        assert_eq!(url, "https://s3.example.com/mybucket/photos/cat.jpg");
    }

    #[test]
    fn host_path_style() {
        let (_, host) = build_url_and_host(
            "https://s3.example.com",
            "s3.example.com",
            "mybucket",
            "photos/cat.jpg",
            true,
        );
        assert_eq!(host, "s3.example.com");
    }

    #[test]
    fn url_virtual_hosted() {
        let (url, _) = build_url_and_host(
            "https://s3.example.com",
            "s3.example.com",
            "mybucket",
            "photos/cat.jpg",
            false,
        );
        assert_eq!(url, "https://mybucket.s3.example.com/photos/cat.jpg");
    }

    #[test]
    fn host_virtual_hosted() {
        let (_, host) = build_url_and_host(
            "https://s3.example.com",
            "s3.example.com",
            "mybucket",
            "photos/cat.jpg",
            false,
        );
        assert_eq!(host, "mybucket.s3.example.com");
    }

    #[test]
    fn canonical_uri_path_style() {
        let uri = build_canonical_uri("mybucket", "photos/cat.jpg", true);
        assert_eq!(uri, "/mybucket/photos/cat.jpg");
    }

    #[test]
    fn canonical_uri_virtual_hosted() {
        let uri = build_canonical_uri("mybucket", "photos/cat.jpg", false);
        assert_eq!(uri, "/photos/cat.jpg");
    }

    // -- XML parsing --

    #[test]
    fn extract_single_value() {
        let xml = "<Key>photos/cat.jpg</Key>";
        assert_eq!(extract_xml_values(xml, "Key"), vec!["photos/cat.jpg"]);
    }

    #[test]
    fn extract_multiple_values() {
        let xml = "<r><Key>a.txt</Key><Key>b.txt</Key></r>";
        assert_eq!(extract_xml_values(xml, "Key"), vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn extract_missing_tag() {
        let xml = "<Bucket>test</Bucket>";
        assert!(extract_xml_values(xml, "Key").is_empty());
    }

    #[test]
    fn extract_empty_value() {
        let xml = "<Key></Key>";
        assert_eq!(extract_xml_values(xml, "Key"), vec![""]);
    }

    #[test]
    fn extract_ignores_unrelated_tags() {
        let xml = "<ListBucketResult><Bucket>test</Bucket><Contents><Key>file.txt</Key></Contents></ListBucketResult>";
        assert_eq!(extract_xml_values(xml, "Key"), vec!["file.txt"]);
        assert_eq!(extract_xml_values(xml, "Bucket"), vec!["test"]);
    }

    #[test]
    fn extract_no_close_tag() {
        let xml = "<Key>broken";
        assert!(extract_xml_values(xml, "Key").is_empty());
    }

    #[test]
    fn extract_single_value_helper() {
        let xml = "<IsTruncated>false</IsTruncated>";
        assert_eq!(
            extract_xml_value(xml, "IsTruncated"),
            Some("false".to_string())
        );
    }

    #[test]
    fn extract_single_value_helper_missing() {
        assert_eq!(extract_xml_value("<a>b</a>", "Key"), None);
    }

    #[test]
    fn endpoint_host_strips_https() {
        assert_eq!(strip_scheme("https://s3.example.com"), "s3.example.com");
    }

    #[test]
    fn endpoint_host_strips_http() {
        assert_eq!(strip_scheme("http://localhost:9000"), "localhost:9000");
    }

    #[test]
    fn endpoint_host_no_scheme() {
        assert_eq!(strip_scheme("s3.example.com"), "s3.example.com");
    }
}
```

Key changes from the original:
- Removed `use http::Uri;`, `use http_body_util::{BodyExt, Full};`
- `client` field is now `reqwest::Client`
- All `hyper::Request::builder()` replaced with `self.client.put/delete/head/get(&url)`
- `signed_headers` applied via `.header()` loop
- Response body read via `.text().await` or `.bytes().await` instead of `.into_body().collect().await.to_bytes()`
- Removed `client()` accessor method (no longer needed)
- All unit tests preserved unchanged (they test pure URL/XML functions)

- [ ] **Step 2: Simplify storage/backend.rs**

Replace `src/storage/backend.rs` with:

```rust
use super::client::RemoteBackend;
use super::memory::MemoryBackend;

pub(crate) enum BackendKind {
    Remote(Box<RemoteBackend>),
    #[cfg_attr(not(any(test, feature = "test-helpers")), allow(dead_code))]
    Memory(MemoryBackend),
}
```

The `http_client()` method is removed — `fetch_url` now uses its own client stored in `StorageInner`.

- [ ] **Step 3: Update storage/facade.rs**

In `src/storage/facade.rs`:

Add a `fetch_client` field to `StorageInner` (after line 89):

Replace:
```rust
pub(crate) struct StorageInner {
    pub(crate) backend: BackendKind,
    pub(crate) public_url: Option<String>,
    pub(crate) max_file_size: Option<usize>,
}
```
→
```rust
pub(crate) struct StorageInner {
    pub(crate) backend: BackendKind,
    pub(crate) public_url: Option<String>,
    pub(crate) max_file_size: Option<usize>,
    pub(crate) fetch_client: Option<reqwest::Client>,
}
```

Update `Storage::with_client()` (line 119) to build the no-redirect fetch client:

Replace:
```rust
    pub fn with_client(config: &BucketConfig, client: crate::http::Client) -> Result<Self> {
        config.validate()?;

        let region = config
            .region
            .clone()
            .unwrap_or_else(|| "us-east-1".to_string());
        let backend = RemoteBackend::new(
            client,
            config.bucket.clone(),
            config.endpoint.clone(),
            config.access_key.clone(),
            config.secret_key.clone(),
            region,
            config.path_style,
        )?;

        Ok(Self {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Remote(Box::new(backend)),
                public_url: config.normalized_public_url(),
                max_file_size: config.max_file_size_bytes()?,
            }),
        })
    }
```
→
```rust
    pub fn with_client(config: &BucketConfig, client: reqwest::Client) -> Result<Self> {
        config.validate()?;

        let region = config
            .region
            .clone()
            .unwrap_or_else(|| "us-east-1".to_string());
        let backend = RemoteBackend::new(
            client,
            config.bucket.clone(),
            config.endpoint.clone(),
            config.access_key.clone(),
            config.secret_key.clone(),
            region,
            config.path_style,
        )?;

        let fetch_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| Error::internal(format!("failed to build fetch HTTP client: {e}")))?;

        Ok(Self {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Remote(Box::new(backend)),
                public_url: config.normalized_public_url(),
                max_file_size: config.max_file_size_bytes()?,
                fetch_client: Some(fetch_client),
            }),
        })
    }
```

Update `Storage::new()` (line 153-155):

Replace:
```rust
    pub fn new(config: &BucketConfig) -> Result<Self> {
        Self::with_client(config, crate::http::Client::default())
    }
```
→
```rust
    pub fn new(config: &BucketConfig) -> Result<Self> {
        Self::with_client(config, reqwest::Client::new())
    }
```

Update `Storage::memory()` — add `fetch_client: None`:

Replace:
```rust
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn memory() -> Self {
        Self {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Memory(MemoryBackend::new()),
                public_url: Some("https://test.example.com".to_string()),
                max_file_size: None,
            }),
        }
    }
```
→
```rust
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn memory() -> Self {
        Self {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Memory(MemoryBackend::new()),
                public_url: Some("https://test.example.com".to_string()),
                max_file_size: None,
                fetch_client: None,
            }),
        }
    }
```

Update `put_from_url_inner()` to use `fetch_client`:

Replace:
```rust
    async fn put_from_url_inner(
        &self,
        input: &PutFromUrlInput,
        opts: &PutOptions,
    ) -> Result<String> {
        let client = self.inner.backend.http_client()?;
        let fetched = fetch_url(client, &input.url, self.inner.max_file_size).await?;
```
→
```rust
    async fn put_from_url_inner(
        &self,
        input: &PutFromUrlInput,
        opts: &PutOptions,
    ) -> Result<String> {
        let client = self
            .inner
            .fetch_client
            .as_ref()
            .ok_or_else(|| Error::internal("URL fetch not supported in memory backend"))?;
        let fetched = fetch_url(client, &input.url, self.inner.max_file_size).await?;
```

Also update the two test structs that construct `StorageInner` manually — add `fetch_client: None` to each:

In `put_respects_max_file_size` test:
```rust
        let storage = Storage {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Memory(MemoryBackend::new()),
                public_url: None,
                max_file_size: Some(5),
                fetch_client: None,
            }),
        };
```

In `url_errors_without_public_url` test:
```rust
        let storage = Storage {
            inner: Arc::new(StorageInner {
                backend: BackendKind::Memory(MemoryBackend::new()),
                public_url: None,
                max_file_size: None,
                fetch_client: None,
            }),
        };
```

- [ ] **Step 4: Rewrite storage/fetch.rs**

Replace `src/storage/fetch.rs` with:

```rust
use std::time::Duration;

use bytes::Bytes;
use http::Uri;

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
/// Hard-coded 30s timeout. No redirect following (caller must pass a
/// no-redirect client).
pub(crate) async fn fetch_url(
    client: &reqwest::Client,
    url: &str,
    max_size: Option<usize>,
) -> Result<FetchResult> {
    validate_url(url)?;

    let mut response = client
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
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

    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| Error::internal(format!("failed to read response body: {e}")))?
    {
        buf.extend_from_slice(&chunk);
        if let Some(max) = max_size
            && buf.len() > max
        {
            return Err(Error::payload_too_large(format!(
                "fetched file size exceeds maximum {max}"
            )));
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

    fn build_test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to build test client")
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
```

Key changes:
- Replaced `hyper::Request` + `Full<Bytes>` + `raw_client()` with `client.get(url)`
- Body streaming uses `response.chunk()` instead of `BodyExt::frame()` loop
- `build_test_client()` creates a no-redirect reqwest client (no `rustls::crypto` init needed)
- All test assertions preserved

- [ ] **Step 5: Commit**

```bash
git add src/storage/client.rs src/storage/backend.rs src/storage/facade.rs src/storage/fetch.rs
git commit -m "refactor: migrate storage module to reqwest"
```

---

### Task 7: Verify compilation

**Files:** None (verification only)

- [ ] **Step 1: Check with all features**

Run: `cargo check --features full,test-helpers`

Expected: Compiles successfully. If there are errors, fix them before proceeding.

Common issues to watch for:
- Missing `use` imports (e.g., `http::HeaderMap` in storage/client.rs — remove if unused)
- `reqwest::Response` vs `http::Response` confusion
- `reqwest::Error` not implementing the right traits for `.chain(e)` — it does, but verify

- [ ] **Step 2: Check clippy**

Run: `cargo clippy --features full,test-helpers --tests -- -D warnings`

Fix any clippy warnings.

- [ ] **Step 3: Check formatting**

Run: `cargo fmt --check`

If needed: `cargo fmt`

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix: resolve compilation issues from reqwest migration"
```

---

### Task 8: Run tests and fix failures

**Files:** Various (depends on test failures)

- [ ] **Step 1: Run all tests**

Run: `cargo test --features full,test-helpers`

- [ ] **Step 2: Run feature-isolated tests**

Run each consumer feature individually:

```bash
cargo test --features auth
cargo test --features storage
cargo test --features webhooks
cargo test --features text-embedding
```

- [ ] **Step 3: Fix any test failures**

Common test issues:
- Webhook tests: reqwest may add extra headers (e.g., `accept: */*`) — tests checking raw HTTP should still pass since they check for specific headers, not exact format
- Storage/fetch tests: streaming behavior might differ — `chunk()` may return data in different chunk sizes than hyper's `frame()`
- `max_size` enforcement: if reqwest returns the entire body in one chunk, the size check still works (checks after each chunk)

- [ ] **Step 4: Commit fixes**

```bash
git add -A
git commit -m "fix: resolve test failures from reqwest migration"
```

---

### Task 9: Update documentation and CI

**Files:**
- Modify: `CLAUDE.md`
- Modify: `README.md`
- Modify: `.github/workflows/ci.yml`
- Delete: `skills/dev/references/http-client.md`
- Modify: `skills/dev/references/config.md`
- Modify: `skills/dev/references/webhooks.md`
- Modify: `skills/dev/references/embed.md`
- Modify: `skills/dev/SKILL.md`
- Modify: `skills/init/references/files.md`
- Modify: `src/config/README.md`
- Modify: `src/auth/README.md`
- Modify: `src/storage/README.md`
- Modify: `src/embed/README.md`

- [ ] **Step 1: Update CLAUDE.md**

In the "Feature Flags" section, remove `http-client` from the list. Change:
```
Feature-gated modules: `db` (default), `session`, `job`, `http-client`, `auth`, `templates`, `sse`, `email`, `storage`, `webhooks`, `dns`, `geolocation`, `qrcode`, `sentry`, `apikey`, `text-embedding`, `tier`.
```
→
```
Feature-gated modules: `db` (default), `session`, `job`, `auth`, `templates`, `sse`, `email`, `storage`, `webhooks`, `dns`, `geolocation`, `qrcode`, `sentry`, `apikey`, `text-embedding`, `tier`.
```

- [ ] **Step 2: Update CI workflow**

In `.github/workflows/ci.yml`, remove `http-client` from the matrix (line 83):

```yaml
        feature: [auth, templates, sse, email, storage, webhooks, dns, geolocation, sentry, test-helpers, session, job, apikey, qrcode, text-embedding]
```

- [ ] **Step 3: Delete http-client skill reference**

```bash
rm skills/dev/references/http-client.md
```

- [ ] **Step 4: Update remaining docs**

For each file that references `http-client`:
- `README.md`: Remove `http-client` from feature lists
- `src/config/README.md`: Remove the `http:` config section documentation
- `src/auth/README.md`: Remove mention of `http-client` dependency
- `src/storage/README.md`: Remove mention of `http-client` dependency
- `src/embed/README.md`: Remove mention of `http-client` dependency
- `skills/dev/references/config.md`: Remove `http:` config section
- `skills/dev/references/webhooks.md`: Update if it references `http-client`
- `skills/dev/references/embed.md`: Update if it references `http-client`
- `skills/dev/SKILL.md`: Remove `http-client` from feature references
- `skills/init/references/files.md`: Remove `http-client` from scaffolding references

In all these files, replace references like `"http-client"` feature or `crate::http::Client` with `reqwest::Client` where applicable.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "docs: remove http-client references, update docs for reqwest migration"
```

---

### Task 10: Final verification and cleanup

- [ ] **Step 1: Full verification**

Run all checks:

```bash
cargo fmt --check
cargo clippy --features full,test-helpers --tests -- -D warnings
cargo test --features full,test-helpers
```

All three must pass clean.

- [ ] **Step 2: Verify dependency tree**

Run: `cargo tree --features full -i reqwest`

Verify reqwest appears only once in the tree (shared between sentry and modo's direct usage).

Run: `cargo tree --features full | grep -E "hyper|hyper-rustls|hyper-util|http-body-util"`

Verify these no longer appear as direct dependencies (they may still appear as transitive deps of reqwest/sentry — that's fine).

- [ ] **Step 3: Final commit if needed**

```bash
git add -A
git commit -m "chore: final cleanup after reqwest migration"
```
