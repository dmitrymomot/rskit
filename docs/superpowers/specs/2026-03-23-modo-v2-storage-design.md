# modo v2 — Storage Module Design

Replaces `src/upload/` (opendal-backed) with `src/storage/` — a custom S3-compatible client using AWS SigV4 signing over raw hyper. Zero new dependencies.

## Motivation

The upload module uses opendal 0.55 for five S3 operations: put, delete, list, exists, presigned URL. opendal is an abstraction layer over 60+ backends — modo uses only S3. Replacing it with a focused custom client:

- Drops opendal and its transitive deps (reqsign, backon, etc.)
- Reuses deps already in the tree (hmac, sha2, hyper, hyper-rustls, hyper-util, http-body-util)
- Enables in-memory presigned URLs (opendal's memory backend errors on presign)
- Gives full control over S3 request construction

## File Layout

```
src/storage/
├── mod.rs          # mod imports + re-exports only
├── backend.rs      # BackendKind enum definition
├── client.rs       # RemoteBackend: hyper HTTP client + SigV4 request construction
├── memory.rs       # MemoryBackend: HashMap<String, StoredObject> behind RwLock
├── signing.rs      # SigV4 signing primitives (canonical request, string-to-sign, derived key, signature)
├── presign.rs      # SigV4 query-string presigned URL generation (pure fn, no HTTP)
├── storage.rs      # Storage: public facade wrapping Arc<StorageInner>
├── buckets.rs      # Buckets: named collection of Storage instances
├── config.rs       # BucketConfig, parse_size, kb/mb/gb
├── options.rs      # PutOptions
├── path.rs         # validate_path, generate_key
└── bridge.rs       # PutInput::from_upload() convenience bridge
```

## Feature Flags

```toml
[features]
full = ["templates", "sse", "auth", "sentry", "email", "storage"]
storage = ["dep:hmac", "dep:hyper", "dep:hyper-rustls", "dep:hyper-util", "dep:http-body-util"]
storage-test = ["storage"]
# upload / upload-test / opendal removed entirely
```

`hmac`, `hyper`, `hyper-rustls`, `hyper-util`, `http-body-util` are already declared as optional deps for `auth`. The `storage` feature activates the same deps — no new crates added.

## Backend Architecture

No trait. An internal enum provides compile-time dispatch:

```rust
// backend.rs
pub(crate) enum BackendKind {
    Remote(RemoteBackend),
    Memory(MemoryBackend),
}
```

`Storage` wraps `Arc<StorageInner>` and matches on `BackendKind` to delegate calls. Both backends implement the same method signatures by convention — no formal trait, no `async_trait`, no `dyn` dispatch.

## SigV4 Signing (`signing.rs`)

AWS Signature Version 4, the standard for all S3-compatible services. Four-step algorithm:

1. **Canonical Request** — method, URI, query string, signed headers, payload hash → SHA-256
2. **String to Sign** — `AWS4-HMAC-SHA256` + datetime + scope + hash of canonical request
3. **Signing Key** — HMAC chain: `"AWS4" + secret` → date → region → `"s3"` → `"aws4_request"`
4. **Signature** — HMAC(signing_key, string_to_sign) → hex string

Public surface (all `pub(crate)`):

```rust
pub(crate) struct SigningParams<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
    pub bucket: &'a str,
    pub key: &'a str,
    pub method: &'a str,
    pub headers: &'a [(String, String)],
    pub payload_hash: &'a str,
    pub now: chrono::DateTime<chrono::Utc>,
}

/// Returns (authorization_header_value, signed_headers_map).
pub(crate) fn sign_request(params: &SigningParams) -> (String, Vec<(String, String)>)
```

Implementation details:
- Uses `hmac 0.12` + `sha2 0.10` (both already in dep tree)
- `chrono::Utc::now()` for timestamps (already in dep tree)
- `uri_encode()` follows AWS spec: encode everything except `A-Za-z0-9_.-~`, slash optionally
- PUT requests use `UNSIGNED-PAYLOAD` for payload hash — avoids double-hashing large uploads
- DELETE/HEAD/GET use SHA-256 of empty string as payload hash
- Tests validate against AWS published test vectors

## Presigned URLs (`presign.rs`)

SigV4 query-string signing variant. Pure function — no HTTP call.

```rust
pub(crate) struct PresignParams<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
    pub bucket: &'a str,
    pub key: &'a str,
    pub endpoint: &'a str,
    pub endpoint_host: &'a str,
    pub path_style: bool,
    pub expires_in: Duration,
    pub now: chrono::DateTime<chrono::Utc>,
}

pub(crate) fn presign_url(params: &PresignParams) -> String
```

Instead of an `Authorization` header, the signature goes into query parameters: `X-Amz-Algorithm`, `X-Amz-Credential`, `X-Amz-Date`, `X-Amz-Expires`, `X-Amz-SignedHeaders`, `X-Amz-Signature`.

## RemoteBackend (`client.rs`)

HTTP client wrapping SigV4-signed requests:

```rust
pub(crate) struct RemoteBackend {
    client: Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    bucket: String,
    endpoint: String,
    endpoint_host: String,
    access_key: String,
    secret_key: String,
    region: String,
    path_style: bool,
}
```

The hyper client is created once in `RemoteBackend::new()` and reused across requests (unlike the OAuth client which builds per-request — storage ops can be frequent).

### URL Construction

Based on `path_style`:

- **`path_style: true`** — `{endpoint}/{bucket}/{key}`, Host: `{endpoint_host}`
- **`path_style: false`** — `https://{bucket}.{endpoint_host}/{key}`, Host: `{bucket}.{endpoint_host}`

### Methods

- **`put`** — PUT request with `Content-Type`, optional `Content-Disposition`, `Cache-Control`. Payload hash: `UNSIGNED-PAYLOAD`.
- **`delete`** — DELETE request. Payload hash: SHA-256 of empty string. Non-existent key returns Ok (S3 semantics).
- **`exists`** — HEAD request. 200 = true, 404 = false, other = error.
- **`list`** — GET `?list-type=2&prefix={prefix}`. Handles pagination via `<IsTruncated>` + `<NextContinuationToken>`. Hand-parses `<Key>` values from XML response (no XML library — simple string extraction). Falls back to `quick-xml` only if hand-parsing proves fragile.
- **`presigned_url`** — delegates to `presign::presign_url()`. No HTTP call.

## MemoryBackend (`memory.rs`)

In-memory storage for unit and integration tests:

```rust
pub(crate) struct MemoryBackend {
    objects: std::sync::RwLock<HashMap<String, StoredObject>>,
    fake_url_base: String,
}

struct StoredObject {
    data: Bytes,
    content_type: String,
}
```

- `std::sync::RwLock` (not tokio) — all ops are synchronous HashMap lookups, never held across `.await`
- `fake_url_base` defaults to `"https://memory.test"`
- `presigned_url()` returns `"https://memory.test/{key}?expires={secs}"` — enables full-flow testing without errors
- `delete()` on missing key is no-op (matches S3 semantics)

## Storage Facade (`storage.rs`)

Public API wrapping `Arc<StorageInner>`:

```rust
pub struct Storage {
    inner: Arc<StorageInner>,
}

struct StorageInner {
    backend: BackendKind,
    public_url: Option<String>,
    max_file_size: Option<usize>,
}
```

### PutInput

Decouples storage from the multipart extractor:

```rust
pub struct PutInput {
    pub data: Bytes,
    pub prefix: String,
    pub filename: Option<String>,
    pub content_type: String,
}
```

### Public Methods

```rust
impl Storage {
    pub fn new(config: &BucketConfig) -> Result<Self>

    #[cfg(any(test, feature = "storage-test"))]
    pub fn memory() -> Self

    pub async fn put(&self, input: &PutInput) -> Result<String>
    pub async fn put_with(&self, input: &PutInput, opts: PutOptions) -> Result<String>
    pub async fn delete(&self, key: &str) -> Result<()>
    pub async fn delete_prefix(&self, prefix: &str) -> Result<()>
    pub fn url(&self, key: &str) -> Result<String>
    pub async fn presigned_url(&self, key: &str, expires_in: Duration) -> Result<String>
    pub async fn exists(&self, key: &str) -> Result<bool>
}
```

- `put`/`put_with` validate `max_file_size`, generate ULID-based key via `path::generate_key()`, delegate to backend. On failure, attempt cleanup delete + log warning.
- `url()` returns `{public_url}/{key}`. Errors if `public_url` not configured.
- `delete_prefix()` calls `backend.list(prefix)` then loops `backend.delete(key)` — same O(n) behavior as opendal's `remove_all()`.
- Manual `Clone` impl (Arc clone).

## Bridge (`bridge.rs`)

Convenience constructor bridging `UploadedFile` → `PutInput`:

```rust
impl PutInput {
    pub fn from_upload(file: &UploadedFile, prefix: &str) -> Self {
        Self {
            data: file.data.clone(),
            prefix: prefix.to_string(),
            filename: Some(file.name.clone()),
            content_type: file.content_type.clone(),
        }
    }
}
```

Usage:

```rust
let key = storage.put(&PutInput::from_upload(&file, "avatars/")).await?;
```

## Buckets (`buckets.rs`)

Named collection of `Storage` instances. Carried from `upload::Buckets` unchanged:

```rust
pub struct Buckets {
    inner: Arc<HashMap<String, Storage>>,
}

impl Buckets {
    pub fn new(configs: &[BucketConfig]) -> Result<Self>
    pub fn get(&self, name: &str) -> Result<Storage>

    #[cfg(any(test, feature = "storage-test"))]
    pub fn memory(names: &[&str]) -> Self
}
```

## Config (`config.rs`)

Carried from `upload::config` with two changes:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BucketConfig {
    pub name: String,
    pub bucket: String,
    pub region: Option<String>,     // was required String → now Option, defaults to us-east-1 in signing
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub public_url: Option<String>,
    pub max_file_size: Option<String>,
    pub path_style: bool,           // new, defaults to true
}
```

- `region: Option<String>` — SigV4 signing defaults to `"us-east-1"` when None. Reduces config friction for S3-compatible services where region is irrelevant.
- `path_style: bool` — `true` (default): `{endpoint}/{bucket}/{key}`. `false`: `https://{bucket}.{endpoint_host}/{key}`. Controls S3 API URL construction only; `public_url` is always `{public_url}/{key}`.
- `validate()`, `parse_size()`, `kb()`, `mb()`, `gb()` carried unchanged.

## Unchanged Files

Carried from `src/upload/` with no changes:

- **`options.rs`** — `PutOptions { content_disposition, cache_control, content_type }`
- **`path.rs`** — `validate_path()`, `generate_key()`

## Re-exports (`mod.rs`)

```rust
mod backend;
mod bridge;
mod buckets;
mod client;
mod config;
mod memory;
mod options;
mod path;
mod presign;
mod signing;
mod storage;

pub use buckets::Buckets;
pub use config::BucketConfig;
pub use config::{gb, kb, mb};
pub use options::PutOptions;
pub use storage::{PutInput, Storage};
```

## Cargo.toml Changes

Added:
```toml
storage = ["dep:hmac", "dep:hyper", "dep:hyper-rustls", "dep:hyper-util", "dep:http-body-util"]
storage-test = ["storage"]
```

Removed:
```toml
upload = ["dep:opendal"]
upload-test = ["upload", "opendal/services-memory"]
opendal = { version = "0.55", optional = true, default-features = false, features = ["services-s3"] }
# and from [dev-dependencies]:
opendal = { version = "0.55", default-features = false, features = ["services-s3", "services-memory"] }
```

Updated:
```toml
full = ["templates", "sse", "auth", "sentry", "email", "storage"]  # upload → storage
```

## Testing Strategy

### Unit Tests (in-crate `#[cfg(test)] mod tests`)

| File | Coverage |
|---|---|
| `signing.rs` | SigV4 canonical request, string-to-sign, signature against AWS published test vectors |
| `presign.rs` | Presigned URL format, expiry, query param ordering, path-style vs virtual-hosted |
| `memory.rs` | All MemoryBackend operations |
| `storage.rs` | Full Storage facade: put, put_with, delete, delete_prefix, url, presigned_url, exists, max_file_size enforcement, path validation |
| `client.rs` | URL construction for path-style and virtual-hosted, Host header logic |
| `config.rs` | parse_size, validation, normalization (carried) |
| `path.rs` | validate_path, generate_key (carried) |
| `bridge.rs` | `PutInput::from_upload()` correctness |

### Integration Tests (`tests/storage.rs`)

```rust
#![cfg(feature = "storage-test")]
```

- Full round-trip: put → exists → url → delete → not exists
- Multi-bucket isolation
- put_with options
- Presigned URL succeeds on memory backend (returns fake URL)

### SigV4 Validation

AWS publishes canonical test vectors for SigV4 signing. These are used in `signing.rs` unit tests to validate correctness without any network calls.

## Supported S3-Compatible Services

All services supporting AWS SigV4 work. Tested configurations:

| Service | `path_style` | Notes |
|---|---|---|
| RustFS | `true` | Local development server |
| MinIO | `true` | Self-hosted |
| AWS S3 | `false` | Virtual-hosted recommended by AWS |
| DigitalOcean Spaces | `false` | Virtual-hosted required for CDN presigned URLs |
| Cloudflare R2 | `true` | Path-style |
| Backblaze B2 | `true` | S3-compatible API |

## Migration Checklist

1. Create `src/storage/` with all files
2. Update `Cargo.toml` — add `storage`/`storage-test`, remove `upload`/`upload-test`/`opendal`
3. Update `src/lib.rs` — replace `pub mod upload` with `pub mod storage`, update re-exports
4. Create `tests/storage.rs`, delete `tests/upload.rs`
5. Delete `src/upload/` directory
6. Update CLAUDE.md — replace upload gotchas with storage gotchas
