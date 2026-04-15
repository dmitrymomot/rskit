# modo::storage

S3-compatible object storage for the modo framework. Supports AWS S3, RustFS,
MinIO, and any provider that implements the S3 API.

Request signing uses AWS Signature Version 4. Both path-style
(`https://endpoint/bucket/key`) and virtual-hosted-style
(`https://bucket.endpoint/key`) URLs are supported.

The memory backend is available inside `#[cfg(test)]` unit-test blocks and
when the `test-helpers` feature is enabled (for integration tests).

## Usage

### Single-bucket setup

```rust,ignore
use modo::storage::{BucketConfig, Storage, PutInput};
use bytes::Bytes;

// `BucketConfig` is `#[non_exhaustive]` — build it from `default()` and assign fields.
let mut config = BucketConfig::default();
config.bucket = "my-bucket".into();
config.endpoint = "https://s3.amazonaws.com".into();
config.access_key = "AKIAIOSFODNN7EXAMPLE".into();
config.secret_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into();
config.region = Some("us-east-1".into());
config.public_url = Some("https://cdn.example.com".into());
config.max_file_size = Some("10mb".into());

let storage = Storage::new(&config)?;

// Using PutInput::new (filename defaults to None — no extension on generated key)
let mut input = PutInput::new(Bytes::from("file contents"), "avatars/", "image/jpeg");

// Set filename to preserve extension on the generated key
input.filename = Some("photo.jpg".into());

let key = storage.put(&input).await?;
let public_url = storage.url(&key)?;
```

### Shared HTTP client

Use `Storage::with_client` to share a `reqwest::Client` connection pool across
multiple `Storage` instances or other modules:

```rust,ignore
use modo::storage::{BucketConfig, Storage};

let client = reqwest::Client::new();
let storage = Storage::with_client(&config, client)?;
```

### Upload with options

```rust,ignore
use modo::storage::{Acl, PutOptions};

let key = storage.put_with(&input, PutOptions {
    content_disposition: Some("attachment".into()),
    cache_control: Some("max-age=31536000".into()),
    acl: Some(Acl::PublicRead),
    ..Default::default()
}).await?;
```

### Upload from a URL

```rust,ignore
use modo::storage::PutFromUrlInput;

// Using the convenience constructor (filename defaults to None)
let mut input = PutFromUrlInput::new("https://example.com/image.png", "downloads/");

// Set filename to preserve extension on the generated key
input.filename = Some("image.png".into());

let key = storage.put_from_url(&input).await?;
```

Redirects are not followed. A hard-coded 30-second timeout applies.
The memory backend returns an error for this operation.

### Retrieving files

`Storage` does not stream bytes back — consumers fetch objects through a URL.
Use `url()` for public objects and `presigned_url()` for private ones.

```rust,ignore
// Public URL (requires `public_url` in BucketConfig). String concatenation only.
let public = storage.url("avatars/01ABC.jpg")?;

// Check existence without downloading (S3 HEAD request).
if storage.exists("avatars/01ABC.jpg").await? {
    // ...
}
```

### Presigned URL

```rust,ignore
use std::time::Duration;

// Signed GET URL, valid for `expires_in`. Works on any backend (including memory).
let url = storage.presigned_url("avatars/01ABC.jpg", Duration::from_secs(3600)).await?;
```

### Delete

```rust,ignore
// Delete a single key (no-op if missing)
storage.delete("avatars/01ABC.jpg").await?;

// Delete all keys under a prefix (O(n) network calls)
storage.delete_prefix("avatars/").await?;
```

### Build from an uploaded file

```rust,ignore
use modo::storage::PutInput;
use modo::extractor::UploadedFile;

let input = PutInput::from_upload(&uploaded_file, "avatars/");
let key = storage.put(&input).await?;
```

### Multi-bucket setup

```rust,ignore
use modo::storage::{BucketConfig, Buckets};

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
```

### In-memory backend for tests

```rust,ignore
// Available in #[cfg(test)] blocks and with the `test-helpers` feature
let storage = Storage::memory();
let buckets = Buckets::memory(&["avatars", "docs"]);
```

## Configuration

`BucketConfig` fields:

| Field           | Type             | Description                                                         |
| --------------- | ---------------- | ------------------------------------------------------------------- |
| `name`          | `String`         | Lookup key used by `Buckets` (not needed for single-bucket use)     |
| `bucket`        | `String`         | S3 bucket name                                                      |
| `endpoint`      | `String`         | S3-compatible endpoint URL                                          |
| `access_key`    | `String`         | Access key ID                                                       |
| `secret_key`    | `String`         | Secret access key                                                   |
| `region`        | `Option<String>` | AWS region; defaults to `us-east-1`                                 |
| `public_url`    | `Option<String>` | Base URL for `Storage::url()`; `None` makes `url()` return an error |
| `max_file_size` | `Option<String>` | Human-readable size limit (e.g. `"10mb"`); `None` disables          |
| `path_style`    | `bool`           | `true` = path-style; `false` = virtual-hosted-style; default `true` |

### Size format

`max_file_size` accepts `b`, `kb`, `mb`, `gb` suffixes (case-insensitive).
The helper functions `kb(n)`, `mb(n)`, and `gb(n)` convert to bytes when
working with sizes in code.

## Key types

| Type              | Description                                         |
| ----------------- | --------------------------------------------------- |
| `Storage`         | Single-bucket handle — upload, delete, URL, presign |
| `Buckets`         | Named collection of `Storage` instances             |
| `PutInput`        | Input for `put()` / `put_with()`                    |
| `PutFromUrlInput` | Input for `put_from_url()` / `put_from_url_with()`  |
| `PutOptions`      | Optional headers and ACL override for uploads       |
| `Acl`             | `Private` (default) or `PublicRead`                 |
| `BucketConfig`    | Deserialisable configuration for one bucket         |
| `kb` / `mb` / `gb`| Size-unit helper functions (convert to bytes)       |
