# Storage

S3-compatible object storage with ACL support and upload-from-URL. Feature-gated under `storage`.

## Feature Flag

```toml
modo = { version = "0.5", features = ["storage"] }
```

Re-exports from `modo` crate root:

```rust
pub use storage::{Acl, BucketConfig, Buckets, PutFromUrlInput, PutInput, PutOptions, Storage};
```

Size helpers `gb`, `kb`, `mb` are available at `modo::storage::{gb, kb, mb}` (not re-exported at crate root).

## Storage

Wraps an `Arc<StorageInner>` -- cheaply cloneable. Constructed from `BucketConfig`.

```rust
let config = BucketConfig {
    bucket: "my-bucket".into(),
    endpoint: "https://s3.us-east-1.amazonaws.com".into(),
    access_key: "AKIA...".into(),
    secret_key: "wJal...".into(),
    region: Some("us-east-1".into()),
    public_url: Some("https://cdn.example.com".into()),
    max_file_size: Some("10mb".into()),
    path_style: true, // default
    ..Default::default()
};
let storage = Storage::new(&config)?;
```

### Constructors

| Constructor    | Signature                                                                      | Notes                                                         |
| -------------- | ------------------------------------------------------------------------------ | ------------------------------------------------------------- |
| `new`          | `pub fn new(config: &BucketConfig) -> Result<Self>`                            | Builds its own default HTTP client                            |
| `with_client`  | `pub fn with_client(config: &BucketConfig, client: reqwest::Client) -> Result<Self>` | Shared connection pool; preferred for multiple `Storage` instances |
| `memory`       | `pub fn memory() -> Self`                                                      | In-memory backend, `#[cfg(test)]` or `test-helpers` feature   |

### Methods

| Method              | Signature                                                                                        | Notes                                                       |
| ------------------- | ------------------------------------------------------------------------------------------------ | ----------------------------------------------------------- |
| `put`               | `async fn put(&self, input: &PutInput) -> Result<String>`                                        | Returns generated S3 key                                    |
| `put_with`          | `async fn put_with(&self, input: &PutInput, opts: PutOptions) -> Result<String>`                 | With custom options                                         |
| `delete`            | `async fn delete(&self, key: &str) -> Result<()>`                                                | No-op if missing                                            |
| `delete_prefix`     | `async fn delete_prefix(&self, prefix: &str) -> Result<()>`                                      | Deletes all keys under prefix                               |
| `url`               | `fn url(&self, key: &str) -> Result<String>`                                                     | Public URL (no network call), requires `public_url`         |
| `presigned_url`     | `async fn presigned_url(&self, key: &str, expires_in: Duration) -> Result<String>`               | Presigned GET URL                                           |
| `exists`            | `async fn exists(&self, key: &str) -> Result<bool>`                                              | HEAD check                                                  |
| `put_from_url`      | `async fn put_from_url(&self, input: &PutFromUrlInput) -> Result<String>`                        | Fetch URL then upload                                       |
| `put_from_url_with` | `async fn put_from_url_with(&self, input: &PutFromUrlInput, opts: PutOptions) -> Result<String>` | With custom options                                         |

### Key Generation

Keys are auto-generated as `{prefix}{ulid}.{ext}` (or `{prefix}{ulid}` if no extension). The ULID is 26 chars. Extension is extracted from `PutInput.filename`.

### Path Validation

All paths (prefixes and keys) are validated: rejects empty strings, leading `/`, `..` segments, and control characters.

### Max File Size

Enforced on both `put()` and `put_from_url()`. Returns `Error::payload_too_large` when exceeded. The `max_file_size` config field accepts human-readable strings: `"10mb"`, `"500kb"`, `"1gb"`, `"1024b"`, or bare bytes `"1024"`.

Size helper functions: `kb(n)`, `mb(n)`, `gb(n)` convert to bytes.

## BucketConfig

`#[non_exhaustive]` — use `..Default::default()` for forward compatibility.

```rust
pub struct BucketConfig {
    pub name: String,            // lookup key in Buckets (ignored by Storage::new())
    pub bucket: String,          // S3 bucket name (required)
    pub region: Option<String>,  // defaults to "us-east-1"
    pub endpoint: String,        // S3-compatible endpoint URL (required)
    pub access_key: String,
    pub secret_key: String,
    pub public_url: Option<String>,    // base URL for url(); None means url() errors
    pub max_file_size: Option<String>, // e.g. "10mb"; None disables limit
    pub path_style: bool,              // defaults to true
}
```

`path_style: true` produces `https://endpoint/bucket/key`. `false` produces `https://bucket.endpoint/key` (virtual-hosted).

Implements `Default` and `Deserialize` (`#[serde(default)]`). Loaded from YAML config.

## Buckets

Named collection of `Storage` instances for multi-bucket apps. Wraps `Arc<HashMap<String, Storage>>`.

```rust
let configs = vec![
    BucketConfig { name: "avatars".into(), bucket: "avatars-bucket".into(), /* ... */ ..Default::default() },
    BucketConfig { name: "docs".into(), bucket: "docs-bucket".into(), /* ... */ ..Default::default() },
];
let buckets = Buckets::new(&configs)?;

let store = buckets.get("avatars")?; // cheap Arc clone
store.put(&input).await?;
```

- Each config must have a non-empty, unique `name`.
- `Buckets::memory(names: &[&str])` for testing (`#[cfg(test)]` or `test-helpers` feature).

## PutInput

`#[non_exhaustive]` — must use a constructor or struct-update syntax.

```rust
pub struct PutInput {
    pub data: Bytes,
    pub prefix: String,              // e.g. "avatars/"
    pub filename: Option<String>,    // used to extract extension
    pub content_type: String,        // MIME type
}
```

### Constructors

Build with `new()` (sets `filename: None`):

```rust
let input = PutInput::new(bytes, "avatars/", "image/jpeg");
```

Build from multipart upload:

```rust
let input = PutInput::from_upload(&uploaded_file, "avatars/");
```

`from_upload` maps an empty `UploadedFile.name` to `filename: None`. Defined in `bridge.rs`.

Set `filename` after construction:

```rust
let mut input = PutInput::new(bytes, "avatars/", "image/jpeg");
input.filename = Some("photo.jpg".into());
```

## PutOptions

`#[non_exhaustive]` — use `..Default::default()` for forward compatibility.

```rust
pub struct PutOptions {
    pub content_disposition: Option<String>, // e.g. "attachment"
    pub cache_control: Option<String>,       // e.g. "max-age=31536000"
    pub content_type: Option<String>,        // overrides PutInput.content_type
    pub acl: Option<Acl>,                    // S3 x-amz-acl header
}
```

All fields default to `None`. Used with `put_with()` and `put_from_url_with()`.

## Acl

`#[non_exhaustive]`, derives `Default` (`Private` is `#[default]`).

```rust
pub enum Acl {
    #[default]
    Private,     // "private"
    PublicRead,  // "public-read"
}
```

`Acl::default()` returns `Acl::Private`. Maps to the S3 `x-amz-acl` header via `acl.as_header_value()`. When `PutOptions.acl` is `None`, the bucket default applies.

## PutFromUrlInput

`#[non_exhaustive]` — must use a constructor or struct-update syntax.

```rust
pub struct PutFromUrlInput {
    pub url: String,              // must be http or https
    pub prefix: String,           // storage prefix
    pub filename: Option<String>, // optional filename hint for extension
}
```

### Constructor

Build with `new()` (sets `filename: None`):

```rust
let input = PutFromUrlInput::new("https://example.com/photo.jpg", "downloads/");
```

Set `filename` after construction:

```rust
let mut input = PutFromUrlInput::new("https://example.com/photo.jpg", "downloads/");
input.filename = Some("photo.jpg".into());
```

Used with `put_from_url()` / `put_from_url_with()`. Fetches the URL, extracts `Content-Type` from the response (falls back to `application/octet-stream`), then uploads via the normal `put` path.

## Upload from URL

```rust
let mut input = PutFromUrlInput::new("https://example.com/photo.jpg", "downloads/");
input.filename = Some("photo.jpg".into());
let key = storage.put_from_url(&input).await?;

// With options:
let key = storage.put_from_url_with(&input, PutOptions {
    acl: Some(Acl::PublicRead),
    ..Default::default()
}).await?;
```

## Internals

- **Signing**: AWS SigV4 signing implemented in `signing.rs`. All S3 requests are signed with HMAC-SHA256.
- **Presigning**: `presign.rs` generates presigned GET URLs with configurable expiry.
- **Backend enum**: `BackendKind::Remote(Box<RemoteBackend>)` for real S3, `BackendKind::Memory(MemoryBackend)` for tests.
- **HTTP client**: Uses `reqwest::Client`. `Storage::new()` creates its own client; `Storage::with_client()` accepts a shared client for connection pooling across multiple `Storage` instances.
- **XML parsing**: Hand-parsed `<Key>` and `<IsTruncated>` tags from ListObjectsV2 responses.
- **Bridge**: `PutInput::from_upload()` bridges the multipart `UploadedFile` extractor to storage input.

## Gotchas

- **S3 keys must be URI-encoded**: All keys are passed through `uri_encode(key, false)` (encodes everything except `A-Za-z0-9_.-~/`). Omitting this breaks keys with spaces, `+`, or other special characters.
- **`delete_prefix()` is O(n) network calls**: Lists all keys under the prefix, then deletes each one individually. Not suitable for large prefixes.
- **Streaming body reads use `response.chunk()` loop**: `fetch_url` reads response bodies chunk-by-chunk via `reqwest::Response::chunk()`, NOT `body.collect().await` -- collect buffers everything, defeating mid-stream abort on size limit.
- **`put_from_url()` does not follow redirects (SSRF prevention)**: Callers must provide the final URL. A 301/302 response is treated as a non-2xx error.
- **30-second hard-coded timeout**: `put_from_url()` sets a per-request timeout via `reqwest::RequestBuilder::timeout(Duration::from_secs(30))`.
- **`x-amz-acl` may be silently ignored by providers**: S3-compatible providers (RustFS/MinIO) may ignore the ACL header if ACLs are disabled at server level.
- **`put_from_url()` on memory backend returns `Error::internal`**: It is inherently a network operation. The memory backend has no HTTP client.
- **URL scheme validation**: Only `http` and `https` URLs are accepted. `ftp`, schemeless, etc. return `Error::bad_request`.
- **Failed uploads are cleaned up**: If `put` to S3 fails, the key is deleted (best-effort) to avoid partial uploads.
- **`Storage` does not implement `Debug`**: Use `.err().unwrap()` not `.unwrap_err()` in tests.
- **Hand-parsed XML**: ListObjectsV2 uses simple string-based XML parsing for `<Key>` extraction. May break with unusual XML structures -- switch to `quick-xml` if needed.
