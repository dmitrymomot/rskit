# Storage ACL + Upload from URL Implementation Plan (Plan 17)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `Acl::Private` / `Acl::PublicRead` support on uploads and a `put_from_url()` method that fetches files from remote URLs with streaming size enforcement.

**Architecture:** `Acl` enum on `PutOptions` flows through to `x-amz-acl` S3 header in `RemoteBackend::put()`. URL fetching is a `fetch_url()` free function in `src/storage/fetch.rs` that reuses the existing hyper client from `RemoteBackend`, streams the response body with size tracking, and returns `(Bytes, content_type)`. The facade's `put_from_url()` / `put_from_url_with()` orchestrate fetch → `put_inner()`.

**Tech Stack:** Existing `hyper` + `hyper-rustls` + `hyper-util` + `http-body-util` (already in `storage` feature gate), `tokio::time::timeout` for fetch timeout.

**Spec:** `docs/superpowers/specs/2026-03-24-modo-v2-storage-acl-url-design.md`

**Reference files:** `src/storage/options.rs`, `src/storage/client.rs`, `src/storage/memory.rs`, `src/storage/facade.rs`, `src/storage/signing.rs`, `src/storage/mod.rs`, `tests/storage.rs`, `tests/webhook_integration.rs` (local HTTP server pattern)

---

### Task 1: Add `Acl` enum and update `PutOptions`

**Files:**
- Modify: `src/storage/options.rs`

- [ ] **Step 1: Write tests for `Acl` enum**

Add to the `#[cfg(test)] mod tests` block in `src/storage/options.rs`:

```rust
use super::*;

#[test]
fn acl_default_is_private() {
    assert_eq!(Acl::default(), Acl::Private);
}

#[test]
fn acl_private_header_value() {
    assert_eq!(Acl::Private.as_header_value(), "private");
}

#[test]
fn acl_public_read_header_value() {
    assert_eq!(Acl::PublicRead.as_header_value(), "public-read");
}

#[test]
fn default_options_acl_is_none() {
    let opts = PutOptions::default();
    assert!(opts.acl.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features storage -p modo storage::options::tests -- --nocapture`
Expected: FAIL — `Acl` type doesn't exist yet.

- [ ] **Step 3: Implement `Acl` enum and update `PutOptions`**

Add to `src/storage/options.rs` before the `PutOptions` struct:

```rust
/// Access control for uploaded objects.
///
/// Maps to the S3 `x-amz-acl` header. `None` in `PutOptions` means
/// the bucket default applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Acl {
    #[default]
    Private,
    PublicRead,
}

impl Acl {
    /// S3 `x-amz-acl` header value.
    pub fn as_header_value(&self) -> &'static str {
        match self {
            Acl::Private => "private",
            Acl::PublicRead => "public-read",
        }
    }
}
```

Add `acl` field to `PutOptions`:

```rust
#[derive(Debug, Clone, Default)]
pub struct PutOptions {
    pub content_disposition: Option<String>,
    pub cache_control: Option<String>,
    pub content_type: Option<String>,
    pub acl: Option<Acl>,
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features storage -p modo storage::options::tests -- --nocapture`
Expected: all 5 tests pass (1 existing + 4 new).

- [ ] **Step 5: Run clippy and check**

Run: `cargo clippy --features storage --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/storage/options.rs
git commit -m "feat(storage): add Acl enum and acl field on PutOptions"
```

---

### Task 2: Wire ACL header in `RemoteBackend::put()`

**Files:**
- Modify: `src/storage/client.rs`

- [ ] **Step 1: Add `x-amz-acl` header to `extra_headers` in `RemoteBackend::put()`**

In `src/storage/client.rs`, in the `put()` method, after the existing `cache_control` push (around line 78), add:

```rust
if let Some(acl) = &opts.acl {
    extra_headers.push(("x-amz-acl".to_string(), acl.as_header_value().to_string()));
}
```

This requires adding `use super::options::Acl;` — but `opts` is already `&PutOptions` which has `acl: Option<Acl>`, so no new import is needed. The field access `opts.acl` works through the existing `PutOptions` import.

- [ ] **Step 2: Run existing tests to verify nothing breaks**

Run: `cargo test --features storage -p modo storage -- --nocapture`
Expected: all existing storage tests pass.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --features storage --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/storage/client.rs
git commit -m "feat(storage): send x-amz-acl header in RemoteBackend::put"
```

---

### Task 3: Store ACL in `MemoryBackend`

**Files:**
- Modify: `src/storage/memory.rs`

- [ ] **Step 1: Write test for ACL storage in memory backend**

Add to `#[cfg(test)] mod tests` in `src/storage/memory.rs`:

```rust
#[tokio::test]
async fn put_stores_acl() {
    let backend = MemoryBackend::new();
    let opts = PutOptions {
        acl: Some(super::super::options::Acl::PublicRead),
        ..Default::default()
    };
    backend
        .put("test/file.txt", Bytes::from("hello"), "text/plain", &opts)
        .await
        .unwrap();

    let map = backend.objects.read().expect("lock poisoned");
    let obj = map.get("test/file.txt").unwrap();
    assert_eq!(obj.acl, Some(super::super::options::Acl::PublicRead));
}

#[tokio::test]
async fn put_stores_none_acl_by_default() {
    let backend = MemoryBackend::new();
    backend
        .put(
            "test/file.txt",
            Bytes::from("hello"),
            "text/plain",
            &PutOptions::default(),
        )
        .await
        .unwrap();

    let map = backend.objects.read().expect("lock poisoned");
    let obj = map.get("test/file.txt").unwrap();
    assert_eq!(obj.acl, None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --features storage -p modo storage::memory::tests -- --nocapture`
Expected: FAIL — `StoredObject` has no `acl` field.

- [ ] **Step 3: Add `acl` field to `StoredObject` and wire it in `put()`**

In `src/storage/memory.rs`, update `StoredObject`:

```rust
use super::options::Acl;

#[allow(dead_code)]
struct StoredObject {
    data: Bytes,
    content_type: String,
    acl: Option<Acl>,
}
```

Update the `put()` method — change the `StoredObject` construction:

```rust
pub async fn put(
    &self,
    key: &str,
    data: Bytes,
    content_type: &str,
    opts: &PutOptions,
) -> Result<()> {
    let mut map = self.objects.write().expect("lock poisoned");
    map.insert(
        key.to_string(),
        StoredObject {
            data,
            content_type: content_type.to_string(),
            acl: opts.acl,
        },
    );
    Ok(())
}
```

Note: the `_opts` parameter name changes to `opts` since we now read from it.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --features storage -p modo storage::memory::tests -- --nocapture`
Expected: all 7 tests pass (5 existing + 2 new).

- [ ] **Step 5: Run clippy**

Run: `cargo clippy --features storage --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add src/storage/memory.rs
git commit -m "feat(storage): store ACL in MemoryBackend for test assertions"
```

---

### Task 4: Add ACL re-export and integration test

**Files:**
- Modify: `src/storage/mod.rs`
- Modify: `src/lib.rs:72`
- Modify: `tests/storage.rs`

- [ ] **Step 1: Add `Acl` re-export**

In `src/storage/mod.rs`, add:

```rust
pub use options::Acl;
```

In `src/lib.rs`, update the storage re-export line (line 72) from:

```rust
pub use storage::{BucketConfig, Buckets, PutInput, PutOptions, Storage};
```

to:

```rust
pub use storage::{Acl, BucketConfig, Buckets, PutInput, PutOptions, Storage};
```

- [ ] **Step 2: Write integration test**

Add to `tests/storage.rs`:

```rust
use modo::storage::Acl;

#[tokio::test]
async fn put_with_acl_public_read() {
    let storage = Storage::memory();
    let input = PutInput {
        data: bytes::Bytes::from("public data"),
        prefix: "public/".into(),
        filename: Some("image.png".into()),
        content_type: "image/png".into(),
    };

    let key = storage
        .put_with(
            &input,
            PutOptions {
                acl: Some(Acl::PublicRead),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert!(storage.exists(&key).await.unwrap());
}

#[tokio::test]
async fn put_with_acl_private() {
    let storage = Storage::memory();
    let input = PutInput {
        data: bytes::Bytes::from("private data"),
        prefix: "private/".into(),
        filename: Some("doc.pdf".into()),
        content_type: "application/pdf".into(),
    };

    let key = storage
        .put_with(
            &input,
            PutOptions {
                acl: Some(Acl::Private),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert!(storage.exists(&key).await.unwrap());
}
```

- [ ] **Step 3: Run integration tests**

Run: `cargo test --features storage-test --test storage -- --nocapture`
Expected: all tests pass (5 existing + 2 new).

- [ ] **Step 4: Fix the existing `put_with_options` test in `tests/storage.rs`**

The existing test on line 76 constructs `PutOptions` without the new `acl` field — since we're using `Default` via `..Default::default()` this is NOT an issue. But the test on line 76 uses a struct literal without `..Default::default()`:

```rust
PutOptions {
    content_disposition: Some("attachment".into()),
    cache_control: Some("no-cache".into()),
    content_type: Some("text/plain".into()),
},
```

This will fail to compile because the new `acl` field is missing. Add `..Default::default()` or `acl: None`:

```rust
PutOptions {
    content_disposition: Some("attachment".into()),
    cache_control: Some("no-cache".into()),
    content_type: Some("text/plain".into()),
    ..Default::default()
},
```

Check all other struct literal sites that construct `PutOptions` without `..Default::default()` — search for `PutOptions {` across the codebase and fix any that are missing the new field.

- [ ] **Step 5: Run full test suite**

Run: `cargo test --features storage-test`
Expected: all tests pass.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy --features storage-test --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/storage/mod.rs src/lib.rs tests/storage.rs
git commit -m "feat(storage): re-export Acl and add ACL integration tests"
```

---

### Task 5: Create `fetch_url()` with URL validation tests

**Files:**
- Create: `src/storage/fetch.rs`
- Modify: `src/storage/mod.rs`

- [ ] **Step 1: Write URL validation tests**

Create `src/storage/fetch.rs`:

```rust
use std::time::Duration;

use bytes::Bytes;
use http::Uri;
use http_body_util::{BodyExt, Full};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;

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

/// Fetch a file from a URL using the provided hyper client.
///
/// Streams the response body and aborts if `max_size` is exceeded.
/// Returns the body bytes and content type from the response.
/// Hard-coded 30s timeout. No redirect following.
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
            if let Some(max) = max_size {
                if buf.len() > max {
                    return Err(Error::payload_too_large(format!(
                        "fetched file size exceeds maximum {max}"
                    )));
                }
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
```

- [ ] **Step 2: Register the module**

In `src/storage/mod.rs`, add:

```rust
mod fetch;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test --features storage -p modo storage::fetch::tests -- --nocapture`
Expected: all 6 tests pass.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --features storage --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add src/storage/fetch.rs src/storage/mod.rs
git commit -m "feat(storage): add fetch_url with URL validation"
```

---

### Task 6: Expose hyper client from `RemoteBackend` and add `put_from_url()` facade

**Files:**
- Modify: `src/storage/client.rs`
- Modify: `src/storage/backend.rs`
- Modify: `src/storage/facade.rs`

- [ ] **Step 1: Write test for `put_from_url()` on memory backend returning error**

Add to `#[cfg(test)] mod tests` in `src/storage/facade.rs`:

```rust
#[tokio::test]
async fn put_from_url_memory_backend_returns_error() {
    let storage = Storage::memory();
    let input = PutFromUrlInput {
        url: "https://example.com/file.jpg".into(),
        prefix: "downloads/".into(),
        filename: Some("file.jpg".into()),
    };
    let err = storage.put_from_url(&input).await.err().unwrap();
    assert_eq!(err.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features storage -p modo storage::facade::tests::put_from_url_memory_backend_returns_error -- --nocapture`
Expected: FAIL — `PutFromUrlInput` doesn't exist yet.

- [ ] **Step 3: Add client accessor to `RemoteBackend`**

In `src/storage/client.rs`, add this method to the `impl RemoteBackend` block:

```rust
pub(crate) fn client(
    &self,
) -> &Client<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    Full<Bytes>,
> {
    &self.client
}
```

- [ ] **Step 4: Add client accessor to `BackendKind`**

In `src/storage/backend.rs`, add these imports and impl block **below** the existing `use` declarations and `BackendKind` enum (do NOT remove the existing `use super::client::RemoteBackend;` and `use super::memory::MemoryBackend;`):

```rust
use bytes::Bytes;
use http_body_util::Full;
use hyper_util::client::legacy::Client;

use crate::error::{Error, Result};

impl BackendKind {
    /// Returns a reference to the hyper HTTP client.
    /// Only available for the Remote backend — Memory returns an error.
    pub(crate) fn http_client(
        &self,
    ) -> Result<
        &Client<
            hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
            Full<Bytes>,
        >,
    > {
        match self {
            BackendKind::Remote(b) => Ok(b.client()),
            BackendKind::Memory(_) => {
                Err(Error::internal("URL fetch not supported in memory backend"))
            }
        }
    }
}
```

- [ ] **Step 5: Add `PutFromUrlInput` struct and `put_from_url()` / `put_from_url_with()` to facade**

In `src/storage/facade.rs`, add the struct after `PutInput`:

```rust
/// Input for `Storage::put_from_url()` and `Storage::put_from_url_with()`.
pub struct PutFromUrlInput {
    /// Source URL to fetch from (must be http or https).
    pub url: String,
    /// Storage prefix (e.g., `"avatars/"`).
    pub prefix: String,
    /// Optional filename hint — used to extract extension. `None` produces extensionless keys.
    pub filename: Option<String>,
}
```

Add the import at the top of `facade.rs`:

```rust
use super::fetch::fetch_url;
```

Add the methods to `impl Storage`:

```rust
/// Fetch a file from a URL and upload it. Returns the generated S3 key.
pub async fn put_from_url(&self, input: &PutFromUrlInput) -> Result<String> {
    self.put_from_url_inner(input, &PutOptions::default()).await
}

/// Fetch a file from a URL and upload it with custom options. Returns the generated S3 key.
pub async fn put_from_url_with(
    &self,
    input: &PutFromUrlInput,
    opts: PutOptions,
) -> Result<String> {
    self.put_from_url_inner(input, &opts).await
}

async fn put_from_url_inner(
    &self,
    input: &PutFromUrlInput,
    opts: &PutOptions,
) -> Result<String> {
    let client = self.inner.backend.http_client()?;
    let fetched = fetch_url(client, &input.url, self.inner.max_file_size).await?;

    let put_input = PutInput {
        data: fetched.data,
        prefix: input.prefix.clone(),
        filename: input.filename.clone(),
        content_type: fetched.content_type,
    };

    self.put_inner(&put_input, opts).await
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test --features storage -p modo storage::facade::tests::put_from_url_memory_backend_returns_error -- --nocapture`
Expected: PASS.

- [ ] **Step 7: Run all storage tests**

Run: `cargo test --features storage -p modo storage -- --nocapture`
Expected: all tests pass.

- [ ] **Step 8: Run clippy**

Run: `cargo clippy --features storage --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 9: Commit**

```bash
git add src/storage/client.rs src/storage/backend.rs src/storage/facade.rs
git commit -m "feat(storage): add put_from_url with hyper client accessor"
```

---

### Task 7: Add re-exports for `PutFromUrlInput`

**Files:**
- Modify: `src/storage/mod.rs`
- Modify: `src/lib.rs:72`

- [ ] **Step 1: Update re-exports**

In `src/storage/mod.rs`, add:

```rust
pub use facade::PutFromUrlInput;
```

In `src/lib.rs`, update the storage re-export line from:

```rust
pub use storage::{Acl, BucketConfig, Buckets, PutInput, PutOptions, Storage};
```

to:

```rust
pub use storage::{Acl, BucketConfig, Buckets, PutFromUrlInput, PutInput, PutOptions, Storage};
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check --features storage`
Expected: compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add src/storage/mod.rs src/lib.rs
git commit -m "feat(storage): re-export PutFromUrlInput"
```

---

### Task 8: HTTP server tests in `fetch.rs` and integration test file

**Files:**
- Modify: `src/storage/fetch.rs`
- Create: `tests/storage_fetch.rs`

Since `fetch_url` is `pub(crate)`, the HTTP server tests go in `src/storage/fetch.rs` unit tests (they can access the function directly). The integration test file only tests the public `Storage` API.

- [ ] **Step 1: Add HTTP server test helpers and tests to `fetch.rs`**

In `src/storage/fetch.rs`, extend the `#[cfg(test)] mod tests` block:

```rust
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

fn build_test_client() -> super::Client<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    Full<Bytes>,
> {
    let connector = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .build();
    Client::builder(TokioExecutor::new()).build(connector)
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
    let (url, handle) =
        start_server(b"pdf content", Some("application/pdf"), 200).await;
    let client = build_test_client();

    let result = fetch_url(&client, &url, None).await.unwrap();
    assert_eq!(result.content_type, "application/pdf");

    handle.await.unwrap();
}
```

- [ ] **Step 2: Create `tests/storage_fetch.rs` for public API tests**

This file tests the public `Storage` API only (memory backend error case):

```rust
#![cfg(feature = "storage-test")]

use http::StatusCode;

use modo::storage::{PutFromUrlInput, Storage};

#[tokio::test]
async fn put_from_url_memory_backend_returns_error() {
    let storage = Storage::memory();
    let input = PutFromUrlInput {
        url: "https://example.com/file.jpg".into(),
        prefix: "downloads/".into(),
        filename: Some("file.jpg".into()),
    };
    let err = storage.put_from_url(&input).await.err().unwrap();
    assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
```

- [ ] **Step 3: Run all tests**

Run: `cargo test --features storage-test -- --nocapture`
Expected: all tests pass — unit tests in `fetch.rs` exercise the HTTP server tests, integration test in `storage_fetch.rs` exercises the public API error path.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --features storage-test --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add src/storage/fetch.rs tests/storage_fetch.rs
git commit -m "test(storage): add fetch_url tests with local HTTP server"
```

---

### Task 9: Update CLAUDE.md with new gotchas

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Add storage ACL + URL fetch notes to CLAUDE.md**

In the `## Current Work` section, update the Plan 17 line to:

```
- **Plan 17 (Storage ACL + Upload from URL):** DONE — `src/storage/` extended with `Acl` enum on `PutOptions`, `x-amz-acl` S3 header, `PutFromUrlInput`, `put_from_url()` / `put_from_url_with()` with streaming fetch and 30s timeout
```

In the `### Storage` gotchas section, add:

```
- `x-amz-acl` may be silently ignored if S3-compatible provider has ACLs disabled — this is provider config, not a framework bug
- `put_from_url()` does not follow redirects (SSRF prevention) — callers must provide the final URL
- `put_from_url()` has a hard-coded 30s timeout — wraps the fetch in `tokio::time::timeout`
- `put_from_url()` on memory backend returns `Error::internal` — it's inherently a network operation, use unit tests in `fetch.rs` for HTTP server tests
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md with storage ACL + URL fetch gotchas"
```

---

### Task 10: Final verification

- [ ] **Step 1: Run full test suite with storage-test feature**

Run: `cargo test --features storage-test`
Expected: all tests pass.

- [ ] **Step 2: Run clippy with all features**

Run: `cargo clippy --features full --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 3: Run format check**

Run: `cargo fmt --check`
Expected: no formatting issues.

- [ ] **Step 4: Verify the new public API is accessible**

Run: `cargo doc --features storage --no-deps 2>&1 | head -5`
Expected: no errors, docs build.
