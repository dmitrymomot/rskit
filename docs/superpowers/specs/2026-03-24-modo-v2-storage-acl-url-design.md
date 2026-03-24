# Plan 17: Storage ACL + Upload from URL

Extend `src/storage/` with two features: ACL control on uploads and fetching files from URLs.

## Decisions

- ACL lives on `PutOptions` (not `PutInput`) — it's a storage-level concern
- Streaming download with early abort on `max_file_size`
- Reuse existing hyper client from `RemoteBackend` for URL fetching
- Reuse `max_file_size` (no separate download limit)
- Content-type from response `Content-Type` header, fallback `application/octet-stream`
- `put_from_url()` accepts `PutOptions` for full control
- Memory backend returns error for `put_from_url()` — it's inherently a network operation
- Hard-coded 30s timeout on URL fetch via `tokio::time::timeout`
- No redirect following — hyper does not follow redirects; callers must provide final URLs
- `opts.content_type` in `PutOptions` overrides auto-detected content type from response header

## 1. ACL Enum

New `Acl` enum in `src/storage/options.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Acl {
    #[default]
    Private,
    PublicRead,
}

impl Acl {
    pub fn as_header_value(&self) -> &'static str {
        match self {
            Acl::Private => "private",
            Acl::PublicRead => "public-read",
        }
    }
}
```

- `PutOptions` gets `pub acl: Option<Acl>` (default `None` = bucket default)
- Re-exported from `mod.rs`

## 2. ACL in Backends

**`RemoteBackend::put()`** — when `opts.acl` is `Some`, push `("x-amz-acl", acl.as_header_value())` into `extra_headers` before signing.

**`MemoryBackend`** — `StoredObject` gets `acl: Option<Acl>` field. Stored from `opts.acl` parameter. Enables test assertions.

## 3. URL Fetching

New file `src/storage/fetch.rs` with:

```rust
pub(crate) struct FetchResult {
    pub data: Bytes,
    pub content_type: String,
}

pub(crate) async fn fetch_url(
    client: &Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    url: &str,
    max_size: Option<usize>,
) -> Result<FetchResult>
```

**Behavior:**
1. Validate URL scheme — must be `http://` or `https://`
2. GET request, no auth, wrapped in `tokio::time::timeout(Duration::from_secs(30), ...)`
3. If timeout: `Error::internal("URL fetch timed out")`
4. If response status is not 2xx: `Error::bad_request("failed to fetch URL ({status})")` — this includes 3xx since hyper does not follow redirects
5. Read `Content-Type` header, default `application/octet-stream`
6. Stream body in chunks, track accumulated size
7. If `max_size` exceeded mid-stream: `Error::payload_too_large(...)`
8. Return `FetchResult { data, content_type }`

**No redirect following:** hyper's `Client` does not follow HTTP redirects. A 301/302 response is treated as a non-2xx error. Callers must provide the final URL. This is intentional — following redirects from user-supplied URLs introduces SSRF risk.

## 4. Facade Methods

New struct in `src/storage/facade.rs`:

```rust
pub struct PutFromUrlInput {
    pub url: String,
    pub prefix: String,
    pub filename: Option<String>,
}
```

Two methods on `Storage`:

```rust
pub async fn put_from_url(&self, input: &PutFromUrlInput) -> Result<String>
pub async fn put_from_url_with(&self, input: &PutFromUrlInput, opts: PutOptions) -> Result<String>
```

**Flow:**
1. Get hyper client from backend (memory backend returns `Error::internal("URL fetch not supported in memory backend")`)
2. `fetch_url(client, &input.url, self.inner.max_file_size)`
3. Build `PutInput` from fetch result + caller's prefix/filename
4. Delegate to `self.put_inner(&put_input, &opts)` — handles `validate_path`, size check, key generation, upload, cleanup on failure

**Client access:** `RemoteBackend` exposes `pub(crate) fn client(&self)` returning a reference to its hyper client.

## 5. Re-exports

`src/storage/mod.rs` adds:
- `pub use options::Acl;`
- `pub use facade::PutFromUrlInput;`

## 6. Error Handling

| Scenario | Constructor | Status |
|---|---|---|
| URL not http/https | `Error::bad_request(...)` | 400 |
| Fetch response not 2xx | `Error::bad_request(...)` | 400 |
| Body exceeds max_file_size mid-stream | `Error::payload_too_large(...)` | 413 |
| Network/connection error | `Error::internal(...)` | 500 |
| put_from_url on memory backend | `Error::internal(...)` | 500 |
| Fetch timeout (30s) | `Error::internal(...)` | 500 |
| S3 PUT fails after fetch | Existing cleanup in `put_inner()` | — |

## 7. Testing

**Unit tests (options.rs):**
- `Acl::default()` is `Private`
- `as_header_value()` returns correct strings
- `PutOptions::default()` has `acl: None`

**Unit tests (fetch.rs):**
- URL validation: rejects `ftp://`, empty, no-scheme

**Unit tests (memory.rs):**
- `StoredObject` stores ACL field

**Unit tests (facade.rs):**
- `put_with()` + `Acl::PublicRead` stores in memory backend
- `put_from_url()` on memory backend returns error

**Unit tests (client.rs):**
- Header-building includes `x-amz-acl` when `opts.acl` is `Some`

**Integration tests (tests/storage_fetch.rs, `#![cfg(feature = "storage-test")]`):**
- Local HTTP server (TcpListener + manual response)
- Successful fetch and store, returns valid key
- Content-type from response header
- Fallback to `application/octet-stream`
- Streaming abort on size limit
- Non-2xx response error
- Redirect (301) returns error (no redirect following)

## 8. Gotchas

- `x-amz-acl` may be silently ignored by S3-compatible providers (e.g., RustFS/MinIO) if ACLs are disabled at the server level — this is provider configuration, not a framework bug
- `as_header_value()` must use explicit match arms (no `_` wildcard) so adding a new `Acl` variant is a compile error
- `fetch_url` imports: `hyper_util::client::legacy::Client`, `hyper_rustls::HttpsConnector`, `hyper_util::client::legacy::connect::HttpConnector` — all behind the `storage` feature gate
- `put_from_url()` returns the generated S3 key (same as `put()`)

## 9. File Changes

**Modified (5):**
- `src/storage/options.rs` — `Acl` enum, `acl` field on `PutOptions`
- `src/storage/client.rs` — `x-amz-acl` header, `client()` accessor
- `src/storage/memory.rs` — `acl` in `StoredObject`
- `src/storage/facade.rs` — `PutFromUrlInput`, `put_from_url()`, `put_from_url_with()`
- `src/storage/mod.rs` — re-exports

**New (2):**
- `src/storage/fetch.rs` — `FetchResult`, `fetch_url()`
- `tests/storage_fetch.rs` — integration tests
