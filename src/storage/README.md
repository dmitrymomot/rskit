# storage

S3-compatible object storage for the `modo` framework. Supports AWS S3, RustFS,
MinIO, and any provider that implements the S3 API.

Request signing uses AWS Signature Version 4. Both path-style
(`https://endpoint/bucket/key`) and virtual-hosted-style
(`https://bucket.endpoint/key`) URLs are supported.

## Feature gate

```toml
[dependencies]
modo = { version = "*", features = ["storage"] }
```

For in-process tests using the memory backend, also add:

```toml
[dev-dependencies]
modo = { version = "*", features = ["storage-test"] }
```

## Usage

### Single-bucket setup

```rust
use modo::storage::{BucketConfig, Storage, PutInput};
use bytes::Bytes;

let config = BucketConfig {
    bucket: "my-bucket".into(),
    endpoint: "https://s3.amazonaws.com".into(),
    access_key: "AKIAIOSFODNN7EXAMPLE".into(),
    secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
    region: Some("us-east-1".into()),
    public_url: Some("https://cdn.example.com".into()),
    max_file_size: Some("10mb".into()),
    path_style: true,
    name: String::new(),
};

let storage = Storage::new(&config)?;

let key = storage.put(&PutInput {
    data: Bytes::from("file contents"),
    prefix: "avatars/".into(),
    filename: Some("photo.jpg".into()),
    content_type: "image/jpeg".into(),
}).await?;

let public_url = storage.url(&key)?;
```

### Upload with options

```rust
use modo::storage::{Acl, PutInput, PutOptions};

let key = storage.put_with(&input, PutOptions {
    content_disposition: Some("attachment".into()),
    cache_control: Some("max-age=31536000".into()),
    acl: Some(Acl::PublicRead),
    ..Default::default()
}).await?;
```

### Upload from a URL

```rust
use modo::storage::PutFromUrlInput;

let key = storage.put_from_url(&PutFromUrlInput {
    url: "https://example.com/image.png".into(),
    prefix: "downloads/".into(),
    filename: Some("image.png".into()),
}).await?;
```

Redirects are not followed. A hard-coded 30-second timeout applies.
The memory backend returns an error for this operation.

### Presigned URL

```rust
use std::time::Duration;

let url = storage.presigned_url("avatars/01ABC.jpg", Duration::from_secs(3600)).await?;
```

### Delete

```rust
// Delete a single key (no-op if missing)
storage.delete("avatars/01ABC.jpg").await?;

// Delete all keys under a prefix (O(n) network calls)
storage.delete_prefix("avatars/").await?;
```

### Build from an uploaded file

```rust
use modo::storage::PutInput;
use modo::extractor::UploadedFile;

let input = PutInput::from_upload(&uploaded_file, "avatars/");
let key = storage.put(&input).await?;
```

### Multi-bucket setup

```rust
use modo::storage::{BucketConfig, Buckets};

let configs = vec![
    BucketConfig { name: "avatars".into(), bucket: "avatars-bucket".into(), /* ... */ ..Default::default() },
    BucketConfig { name: "docs".into(),    bucket: "docs-bucket".into(),    /* ... */ ..Default::default() },
];
let buckets = Buckets::new(&configs)?;

let avatars = buckets.get("avatars")?;
```

### In-memory backend for tests

```rust
// Requires the `storage-test` feature
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
