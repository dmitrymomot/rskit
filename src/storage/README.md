# modo::storage

S3-compatible object storage for the modo framework. Works with AWS S3,
RustFS, MinIO, or any provider that speaks the S3 API.

Requests are signed with AWS Signature Version 4. Both path-style
(`https://endpoint/bucket/key`) and virtual-hosted-style
(`https://bucket.endpoint/key`) URLs are supported via the `path_style`
flag on `BucketConfig`.

A memory backend is available inside `#[cfg(test)]` blocks and when the
`test-helpers` feature is enabled (for integration tests). It supports every
operation except `put_from_url`, which always errors on the memory backend.

## Key types

| Type               | Description                                                     |
| ------------------ | --------------------------------------------------------------- |
| `Storage`          | Single-bucket handle — upload, delete, URL, presign, exists     |
| `Buckets`          | Named collection of `Storage` instances for multi-bucket apps   |
| `PutInput`         | Input for `Storage::put` / `Storage::put_with`                  |
| `PutFromUrlInput`  | Input for `Storage::put_from_url` / `Storage::put_from_url_with`|
| `PutOptions`       | Optional headers + ACL override applied to an upload            |
| `Acl`              | `Private` (default) or `PublicRead` — mapped to `x-amz-acl`     |
| `BucketConfig`     | Deserialisable configuration for one bucket                     |
| `kb` / `mb` / `gb` | Size-unit helpers returning `usize` bytes                       |

Backends: an S3-compatible remote backend (via `Storage::new` /
`Storage::with_client`) and an in-memory backend (via `Storage::memory` /
`Buckets::memory`, gated by `#[cfg(test)]` or `test-helpers`). No filesystem
backend is provided.

## S3 key encoding

Pass raw (unencoded) keys to every `Storage` method. Before a key is placed
into a signed request, it is:

1. Validated — empty strings, a leading `/`, any `..` path segment, and any
   ASCII control character are rejected with
   `Error::bad_request`.
2. URI-encoded with AWS rules via `uri_encode(key, encode_slash = false)`
   so `/` stays a path separator and every reserved byte is percent-encoded.

The key returned by `put` / `put_with` / `put_from_url` / `put_from_url_with`
is the raw (unencoded) key — feed it back to `delete`, `url`, `exists`, or
`presigned_url` as-is. Do not pre-encode.

## Usage

### Single-bucket setup

```rust,no_run
use bytes::Bytes;
use modo::storage::{BucketConfig, PutInput, Storage};

# async fn run() -> modo::Result<()> {
// BucketConfig is #[non_exhaustive] — build from default() and assign fields.
let mut config = BucketConfig::default();
config.bucket = "my-bucket".into();
config.endpoint = "https://s3.amazonaws.com".into();
config.access_key = "AKIAIOSFODNN7EXAMPLE".into();
config.secret_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into();
config.region = Some("us-east-1".into());
config.public_url = Some("https://cdn.example.com".into());
config.max_file_size = Some("10mb".into());

let storage = Storage::new(&config)?;

// PutInput::new leaves filename as None (no extension on the generated key).
let mut input = PutInput::new(Bytes::from_static(b"file contents"), "avatars/", "image/jpeg");
// Set filename to carry the extension onto the generated key.
input.filename = Some("photo.jpg".into());

let key = storage.put(&input).await?;
let public = storage.url(&key)?;
# let _ = public;
# Ok(()) }
```

The generated key is `"{prefix}{ulid}"` or `"{prefix}{ulid}.{ext}"` — 26-char
ULID plus the lower-cased extension from `filename`, if any.

### Shared HTTP client

`Storage::with_client` shares a `reqwest::Client` connection pool across
multiple `Storage` instances (or other modules). URL fetching for
`put_from_url` always uses a separate internal client with redirects
disabled and a 30-second timeout.

```rust,no_run
use modo::storage::{BucketConfig, Storage};

# fn run(config: &BucketConfig) -> modo::Result<()> {
let client = reqwest::Client::new();
let storage = Storage::with_client(config, client)?;
# let _ = storage;
# Ok(()) }
```

### Upload with options

```rust,no_run
use modo::storage::{Acl, PutInput, PutOptions, Storage};

# async fn run(storage: &Storage, input: &PutInput) -> modo::Result<()> {
let key = storage
    .put_with(
        input,
        PutOptions {
            content_disposition: Some("attachment".into()),
            cache_control: Some("max-age=31536000".into()),
            acl: Some(Acl::PublicRead),
            ..Default::default()
        },
    )
    .await?;
# let _ = key;
# Ok(()) }
```

### Upload from a URL

```rust,no_run
use modo::storage::{PutFromUrlInput, Storage};

# async fn run(storage: &Storage) -> modo::Result<()> {
let mut input = PutFromUrlInput::new("https://example.com/image.png", "downloads/");
input.filename = Some("image.png".into());
let key = storage.put_from_url(&input).await?;
# let _ = key;
# Ok(()) }
```

Redirects are not followed; a hard-coded 30-second timeout applies; the
memory backend always errors on this operation.

### Retrieving files

`Storage` does not stream bytes back — consumers fetch objects through a
URL. Use `url()` for public objects and `presigned_url()` for private ones.

```rust,no_run
use std::time::Duration;
use modo::storage::Storage;

# async fn run(storage: &Storage) -> modo::Result<()> {
// Public URL — requires `public_url` in BucketConfig; plain string join, no I/O.
let public = storage.url("avatars/01ABC.jpg")?;

// Existence check — S3 HEAD request.
if storage.exists("avatars/01ABC.jpg").await? {
    // ...
}

// Presigned GET URL valid for `expires_in`.
let signed = storage
    .presigned_url("avatars/01ABC.jpg", Duration::from_secs(3600))
    .await?;
# let _ = (public, signed);
# Ok(()) }
```

### Delete

```rust,no_run
use modo::storage::Storage;

# async fn run(storage: &Storage) -> modo::Result<()> {
// Single key — no-op if missing.
storage.delete("avatars/01ABC.jpg").await?;

// All keys under a prefix — O(n) network calls (LIST + one DELETE per key).
storage.delete_prefix("avatars/").await?;
# Ok(()) }
```

### Build from an uploaded file

```rust,no_run
use modo::extractor::UploadedFile;
use modo::storage::{PutInput, Storage};

# async fn run(storage: &Storage, uploaded: &UploadedFile) -> modo::Result<()> {
let input = PutInput::from_upload(uploaded, "avatars/");
let key = storage.put(&input).await?;
# let _ = key;
# Ok(()) }
```

### Multi-bucket setup

```rust,no_run
use modo::storage::{BucketConfig, Buckets};

# fn run() -> modo::Result<()> {
let mut avatars_cfg = BucketConfig::default();
avatars_cfg.name = "avatars".into();
avatars_cfg.bucket = "avatars-bucket".into();
avatars_cfg.endpoint = "https://s3.amazonaws.com".into();
// ... access_key, secret_key, etc.

let mut docs_cfg = BucketConfig::default();
docs_cfg.name = "docs".into();
docs_cfg.bucket = "docs-bucket".into();
docs_cfg.endpoint = "https://s3.amazonaws.com".into();

let buckets = Buckets::new(&[avatars_cfg, docs_cfg])?;
let avatars = buckets.get("avatars")?;
# let _ = avatars;
# Ok(()) }
```

`Buckets::new` rejects empty or duplicated `name` fields.

### In-memory backend for tests

```rust,ignore
// Available in #[cfg(test)] blocks and under the `test-helpers` feature.
let storage = modo::storage::Storage::memory();
let buckets = modo::storage::Buckets::memory(&["avatars", "docs"]);
```

## Configuration

`BucketConfig` deserialises cleanly from YAML. Fields:

| Field           | Type             | Description                                                         |
| --------------- | ---------------- | ------------------------------------------------------------------- |
| `name`          | `String`         | Lookup key used by `Buckets` (ignored by a single-bucket `Storage`) |
| `bucket`        | `String`         | S3 bucket name — required                                           |
| `endpoint`      | `String`         | S3-compatible endpoint URL — required                               |
| `access_key`    | `String`         | Access key ID                                                       |
| `secret_key`    | `String`         | Secret access key                                                   |
| `region`        | `Option<String>` | AWS region; defaults to `us-east-1`                                 |
| `public_url`    | `Option<String>` | Base URL for `Storage::url`; `None` makes `url()` return an error   |
| `max_file_size` | `Option<String>` | Human-readable size limit (e.g. `"10mb"`); `None` disables          |
| `path_style`    | `bool`           | `true` (default) = path-style; `false` = virtual-hosted-style       |

### Example YAML

```yaml
storage:
  bucket: my-app-uploads
  endpoint: ${S3_ENDPOINT:https://s3.amazonaws.com}
  access_key: ${S3_ACCESS_KEY}
  secret_key: ${S3_SECRET_KEY}
  region: us-east-1
  public_url: https://cdn.example.com
  max_file_size: 10mb
  path_style: true
```

For multiple buckets the YAML is a list where each entry must set a unique,
non-empty `name`:

```yaml
buckets:
  - name: avatars
    bucket: my-app-avatars
    endpoint: ${S3_ENDPOINT}
    access_key: ${S3_ACCESS_KEY}
    secret_key: ${S3_SECRET_KEY}
    public_url: https://cdn.example.com/avatars
    max_file_size: 5mb
  - name: docs
    bucket: my-app-docs
    endpoint: ${S3_ENDPOINT}
    access_key: ${S3_ACCESS_KEY}
    secret_key: ${S3_SECRET_KEY}
    max_file_size: 50mb
```

### Size format

`max_file_size` accepts a number with an optional suffix — `b`, `kb`, `mb`,
`gb` (case-insensitive). A bare number is interpreted as bytes. Zero values
are rejected at validation time. In code, `kb(n)`, `mb(n)`, and `gb(n)`
return the matching `usize` byte count.
