# modo v2 — Upload Module Design

S3-compatible storage via OpenDAL with presigned URLs, multi-bucket support, and in-memory testing.

## Design Decisions

- **S3-only, no local filesystem backend.** All storage goes through OpenDAL's S3 service. Dev/test environments use `Storage::memory()` (OpenDAL Memory backend) or a local S3-compatible service like MinIO.
- **Concrete `Storage` type, not a trait.** The `opendal::Operator` itself is the abstraction — swapping backends (S3 vs Memory) happens at operator construction, not via trait dispatch.
- **Multi-bucket via `Buckets` map.** `Storage` is a single bucket. `Buckets` holds named `Storage` instances for apps that need multiple buckets.
- **Per-handler validation, no global allowed_types.** Each handler validates file type/size using `UploadedFile::validate().accept().max_size().check()`. Different endpoints can accept different types.
- **Pre-configured ACL, not per-object.** `default_acl` is set at the operator level (one ACL per `Storage` instance). Per-object ACL is not abstracted — provider support is too fragmented (R2, Hetzner, B2, MinIO don't support it; OpenDAL has no per-write ACL field).
- **Presigned URLs via OpenDAL `presign_read()`.** Works with any S3-compatible service. Async but no network round-trip for S3 backends.
- **`upload-test` feature** for `Storage::memory()` / `Buckets::memory()` constructors, mirroring the `email-test` pattern.

## Config

```yaml
# Single bucket
upload:
  name: assets                              # ignored by Storage::new(), used by Buckets
  bucket: my-app-uploads
  region: us-east-1
  endpoint: https://s3.example.com
  access_key: ${S3_ACCESS_KEY}
  secret_key: ${S3_SECRET_KEY}
  public_url: https://cdn.example.com       # optional, for url()
  default_acl: public-read                  # optional, applied to all writes
  max_file_size: 10mb                       # optional, global default

# Multiple buckets
upload:
  - name: avatars
    bucket: my-avatars-bucket
    region: us-east-1
    endpoint: https://s3.example.com
    access_key: ${S3_KEY}
    secret_key: ${S3_SECRET}
    public_url: https://cdn.example.com
    default_acl: public-read
    max_file_size: 5mb

  - name: documents
    bucket: my-docs-bucket
    region: us-east-1
    endpoint: https://s3.example.com
    access_key: ${S3_KEY}
    secret_key: ${S3_SECRET}
    default_acl: private
    max_file_size: 50mb
```

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BucketConfig {
    pub name: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub public_url: Option<String>,
    pub default_acl: Option<String>,
    pub max_file_size: Option<String>,
}
```

### Config Validation (at construction time)

- `bucket` must not be empty.
- `endpoint` must not be empty.
- `max_file_size` if set, must parse to > 0 bytes.
- `public_url: Some("")` normalized to `None`.
- Invalid config = error at startup, not at request time.

## Storage API

### Constructors

```rust
// Production — single bucket from config
let storage = Storage::new(&config)?;
registry.add(storage);

// Multiple named buckets
let buckets = Buckets::new(&vec_of_configs)?;
registry.add(buckets);

// Testing (behind upload-test feature)
let storage = Storage::memory();
let buckets = Buckets::memory(&["avatars", "docs"]);
```

### Storage Methods

```rust
impl Storage {
    /// Upload a file under prefix/. Returns the S3 key.
    /// Validates max_file_size if configured.
    pub async fn put(&self, file: &UploadedFile, prefix: &str) -> Result<String>;

    /// Upload with options (content-disposition, cache-control, content-type override).
    pub async fn put_with(&self, file: &UploadedFile, prefix: &str, opts: PutOptions) -> Result<String>;

    /// Delete a single object by key.
    pub async fn delete(&self, key: &str) -> Result<()>;

    /// Delete all objects under a prefix.
    pub async fn delete_prefix(&self, prefix: &str) -> Result<()>;

    /// Public URL (string concatenation, no network call).
    /// Returns Error if public_url is not configured.
    pub fn url(&self, key: &str) -> Result<String>;

    /// Presigned URL via OpenDAL. Works with any S3-compatible service.
    pub async fn presigned_url(&self, key: &str, expires_in: Duration) -> Result<String>;

    /// Check if a key exists.
    pub async fn exists(&self, key: &str) -> Result<bool>;
}
```

### Buckets

```rust
impl Buckets {
    /// Create from a list of bucket configs. Errors on duplicate names.
    pub fn new(configs: &[BucketConfig]) -> Result<Self>;

    /// Get a Storage by name. Errors if not found.
    pub fn get(&self, name: &str) -> Result<&Storage>;

    /// Testing: create named in-memory buckets.
    #[cfg(feature = "upload-test")]
    pub fn memory(names: &[&str]) -> Self;
}
```

### PutOptions

```rust
#[derive(Debug, Clone, Default)]
pub struct PutOptions {
    pub content_disposition: Option<String>,
    pub cache_control: Option<String>,
    pub content_type: Option<String>,
}
```

### Usage Examples

```rust
// Single bucket
async fn upload_avatar(
    Service(storage): Service<Storage>,
    MultipartRequest(_, mut files): MultipartRequest<Form>,
) -> Result<Json<String>> {
    let file = files.file("avatar").ok_or_else(|| Error::bad_request("missing avatar"))?;
    file.validate()
        .max_size(mb(5))
        .accept("image/*")
        .check()?;
    let key = storage.put(&file, "avatars/").await?;
    Ok(Json(storage.url(&key)?))
}

// Multiple buckets
async fn upload_document(
    Service(buckets): Service<Buckets>,
    MultipartRequest(_, mut files): MultipartRequest<Form>,
) -> Result<Json<String>> {
    let file = files.file("doc").ok_or_else(|| Error::bad_request("missing doc"))?;
    file.validate()
        .max_size(mb(50))
        .accept("application/pdf")
        .check()?;
    let store = buckets.get("documents")?;
    let key = store.put(&file, "reports/").await?;
    Ok(Json(key))
}

// Upload with options
let key = storage.put_with(&file, "downloads/", PutOptions {
    content_disposition: Some("attachment".into()),
    cache_control: Some("max-age=31536000".into()),
    ..Default::default()
}).await?;

// Presigned URL for private file access
let url = storage.presigned_url(&key, Duration::from_secs(3600)).await?;
```

## Internals

### Storage Inner

```rust
pub struct Storage {
    inner: Arc<StorageInner>,
}

struct StorageInner {
    operator: opendal::Operator,
    public_url: Option<String>,
    max_file_size: Option<usize>,
}
```

- `Storage` is cheaply cloneable (`Arc`).
- `Buckets` wraps `Arc<HashMap<String, Storage>>`.

### File Key Generation

Format: `{prefix}{ulid}.{ext}`

```
storage.put(&file, "avatars/")
→ "avatars/01JQXK3M7N8P9R2S4T5V6W.jpg"
```

- Uses `id::ulid()` (26 chars) — time-sortable, unique.
- Extension from `UploadedFile::extension()` (lowercase).
- No extension → no dot appended.

### Path Validation

Applied to prefix (in `put`) and key (in `delete`, `url`, `presigned_url`, `exists`):

- Reject `..` anywhere in the path.
- Reject absolute paths (leading `/`).
- Reject empty prefix.
- Reject control characters.

### put() Flow

1. Validate prefix (path safety).
2. Check `max_file_size` if configured → `Error::payload_too_large()`.
3. Generate key: `{prefix}{ulid}.{ext}`.
4. Write via `operator.write()` or `operator.write_with()` (if PutOptions).
5. On write failure: best-effort `operator.delete()` cleanup.
6. Return the key as `String`.

### Network Call Summary

| Method | Network? | Notes |
|---|---|---|
| `put()` / `put_with()` | Yes | Writes to S3 |
| `delete()` | Yes | Deletes from S3 |
| `delete_prefix()` | Yes | Lists + deletes |
| `exists()` | Yes | HEAD request |
| `url()` | No | String concatenation |
| `presigned_url()` | No* | OpenDAL computes locally for S3 |

*OpenDAL's S3 presign is local computation, but the method is async for API consistency.

## Error Handling

| Scenario | Error |
|---|---|
| File exceeds `max_file_size` | `Error::payload_too_large()` |
| Path traversal (`..`, leading `/`) | `Error::bad_request()` |
| `url()` without `public_url` configured | `Error::internal("public_url not configured")` |
| `buckets.get("unknown")` | `Error::internal("bucket 'unknown' not configured")` |
| OpenDAL write/delete/presign failure | `Error::internal(format!(...))` |
| Duplicate `name` in `Buckets::new()` | `Error::internal("duplicate bucket name '...'")` |
| Empty `bucket` or `endpoint` in config | `Error::internal("bucket name is required")` |

### Tracing

- `put()` / `put_with()`: `info!(key, size, "file uploaded")`
- `delete()`: `info!(key, "file deleted")`
- `delete_prefix()`: `info!(prefix, "prefix deleted")`
- Cleanup failure: `warn!(key, error, "failed to clean up partial upload")`

## Module Structure

```
src/upload/
    mod.rs          -- mod imports, re-exports
    config.rs       -- BucketConfig
    storage.rs      -- Storage (single bucket)
    buckets.rs      -- Buckets (named map)
    options.rs      -- PutOptions
    path.rs         -- key generation, path validation
```

### Feature Gate

```toml
[features]
upload = ["dep:opendal"]
upload-test = ["upload"]

[dependencies]
opendal = { version = "0.55", optional = true, features = ["services-s3"] }

[dev-dependencies]
opendal = { version = "0.55", features = ["services-s3", "services-memory"] }
```

- `upload` gates all upload code + opendal dependency.
- `upload-test` enables `Storage::memory()` and `Buckets::memory()`.
- Dev-dependencies always have `services-memory` for in-crate tests.
- Lint/test commands: `cargo test --features upload`, `cargo clippy --features upload --tests`.

### Re-exports

```rust
// modo::upload
pub use config::BucketConfig;
pub use storage::Storage;
pub use buckets::Buckets;
pub use options::PutOptions;
```

### Existing Code (no changes needed)

- `UploadedFile` stays in `src/extractor/multipart.rs`.
- `Files`, `MultipartRequest` stay in extractor module.
- Validation (`validate().max_size().accept().check()`) stays in extractor.
- `Storage::put()` accepts `&UploadedFile` — cross-module reference.

## Testing Strategy

### In-crate unit tests

**`config.rs`:**
- Default config values.
- Validation rejects empty bucket/endpoint.
- Validation rejects invalid max_file_size.
- `public_url: Some("")` normalized to `None`.

**`path.rs`:**
- Key generation: correct format `{prefix}{ulid}.{ext}`.
- Extension preserved, lowercased.
- No extension → no dot.
- Path validation rejects `..`, leading `/`, empty prefix, control chars.

**`options.rs`:**
- `PutOptions::default()` — all `None`.

**`storage.rs`** (using Memory backend via dev-deps):
- `put()` stores and returns key matching expected format.
- `put()` respects max_file_size → payload_too_large error.
- `put_with()` passes options through.
- `delete()` removes file, subsequent `exists()` returns false.
- `delete()` on non-existent key — verify behavior.
- `delete_prefix()` removes all under prefix.
- `url()` returns `{public_url}/{key}`.
- `url()` errors when `public_url` is `None`.
- `url()` handles trailing slash in `public_url`.
- `presigned_url()` — test that Memory backend returns a clear error or URL.
- `exists()` true after put, false after delete.

**`buckets.rs`:**
- `Buckets::new()` creates entries, `get()` returns correct storage.
- `get()` with unknown name returns error.
- Duplicate name in config returns error.
- Empty config vec is valid (zero buckets).

### Integration tests (`tests/upload.rs`)

- `#![cfg(feature = "upload")]` at top.
- Full round-trip: put → exists → url → delete → not exists.
- Multi-bucket via `Buckets`: put to different buckets, verify isolation.

## Gotchas

- `upload` feature required: `cargo test --features upload`, `cargo clippy --features upload --tests`.
- `Storage::memory()` / `Buckets::memory()` only available with `upload-test` feature.
- `presigned_url()` may error on Memory backend (no signing support) — tests should handle this.
- `opendal::Operator` is `Clone` (wraps `Arc` internally) — `Storage` still uses its own `Arc<StorageInner>` because it holds extra fields (`public_url`, `max_file_size`).
- OpenDAL `WriteOptions` has no per-write ACL field — ACL is set once via `S3::default_acl()` at operator construction.
- Provider ACL support varies: R2/Hetzner/B2/MinIO don't support per-object ACL. `default_acl` config is best-effort.
