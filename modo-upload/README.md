# modo-upload

[![docs.rs](https://img.shields.io/docsrs/modo-upload)](https://docs.rs/modo-upload)

File upload support for modo applications: multipart form parsing, pluggable
storage backends, and per-field validation — all driven by a single derive macro.

## Features

| Feature   | Default | Description                                           |
| --------- | ------- | ----------------------------------------------------- |
| `local`   | yes     | Local filesystem storage via `LocalStorage`           |
| `opendal` | no      | S3-compatible object storage via OpenDAL (`S3Config`) |

## Usage

### Define a form struct

Derive `FromMultipart` on a named-field struct. Each field maps to a multipart
field by its Rust name (or `#[serde(rename = "...")]`).

```rust
use modo_upload::{FromMultipart, UploadedFile};

#[derive(FromMultipart)]
struct ProfileForm {
    // accept only images up to 5 MB
    #[upload(max_size = "5mb", accept = "image/*")]
    avatar: UploadedFile,

    // optional second file
    banner: Option<UploadedFile>,

    // multiple file upload (1–4 files, each at most 2 MB)
    #[upload(min_count = 1, max_count = 4, max_size = "2mb")]
    gallery: Vec<UploadedFile>,

    // plain text field
    name: String,

    // optional text field
    bio: Option<String>,
}
```

Supported field types:

| Rust type              | Multipart field                                                     |
| ---------------------- | ------------------------------------------------------------------- |
| `UploadedFile`         | required file                                                       |
| `Option<UploadedFile>` | optional file                                                       |
| `Vec<UploadedFile>`    | zero or more files (count constraints via `min_count`/`max_count`) |
| `BufferedUpload`       | required file (chunked reader; at most one per struct)              |
| `String`               | required text                                                       |
| `Option<String>`       | optional text                                                       |
| any `FromStr` type     | required text, parsed via `FromStr`                                 |

### Extract in a handler

Use `MultipartForm<T>` as an axum extractor. Text fields are auto-sanitized.
Call `.validate()` when `T` also derives `modo::Validate`.

```rust
use std::sync::Arc;
use modo::{Json, JsonResult, Service};
use modo_upload::{FileStorageDyn, MultipartForm};

#[modo::handler(POST, "/profile")]
async fn update_profile(
    storage: Service<Arc<dyn FileStorageDyn>>,
    form: MultipartForm<ProfileForm>,
) -> JsonResult<serde_json::Value> {
    form.validate()?;
    let stored = storage.store("avatars", &form.avatar).await?;
    Ok(Json(serde_json::json!({
        "name": form.name,
        "avatar_path": stored.path,
    })))
}
```

### Register the storage backend

Build the storage backend from `UploadConfig` and register it as a service so
extractors can resolve it via `Service<Arc<dyn FileStorageDyn>>`.

```rust
use modo_upload::{UploadConfig, storage};
use serde::Deserialize;

#[derive(Default, Deserialize)]
struct AppConfig {
    #[serde(flatten)]
    core: modo::AppConfig,
    #[serde(default)]
    upload: UploadConfig,
}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let file_storage = storage(&config.upload)?;
    app.config(config.core).service(file_storage).run().await
}
```

### Manual file validation

`UploadedFile` exposes a fluent `validate()` builder when you need validation
outside the derive macro:

```rust
use modo_upload::{UploadedFile, mb};

fn check(file: &UploadedFile) -> Result<(), modo::Error> {
    file.validate()
        .max_size(mb(10))
        .accept("image/*")
        .check()
}
```

Size helper functions: `kb(n)`, `mb(n)`, `gb(n)` — return bytes as `usize`.

## Configuration

`UploadConfig` is deserialized from the application config file under an
`upload` key:

```yaml
upload:
  backend: local      # "local" (default) or "s3"
  path: ./uploads     # base directory for local backend
  max_file_size: 10mb # global default; per-field #[upload(max_size)] overrides
```

S3 configuration (requires the `opendal` feature):

```yaml
upload:
  backend: s3
  s3:
    bucket: my-bucket
    region: us-east-1
    endpoint: ""        # leave empty for AWS; set for MinIO etc.
    access_key_id: AKIA...
    secret_access_key: secret
```

`S3Config` fields: `bucket`, `region`, `endpoint`, `access_key_id`,
`secret_access_key` — all `String`, all default to empty.

## Key Types

| Type                             | Description                                                              |
| -------------------------------- | ------------------------------------------------------------------------ |
| `FromMultipart` (trait + derive) | Parse `multipart/form-data` into a struct                                |
| `MultipartForm<T>`               | Axum extractor; wraps a `FromMultipart` type                             |
| `UploadedFile`                   | In-memory file with name, content-type, and bytes                        |
| `BufferedUpload`                 | In-memory file; provides a chunked reader via `into_reader()`            |
| `FileStorage`                    | Async trait for storing, deleting, and querying files                    |
| `FileStorageDyn`                 | Object-safe companion; use `Arc<dyn FileStorageDyn>` in handlers         |
| `StoredFile`                     | Result of a store operation: `path` and `size`                           |
| `UploadConfig`                   | Deserialized upload configuration                                        |
| `StorageBackend`                 | Enum: `Local` or `S3`                                                    |
| `storage()`                      | Factory function: `&UploadConfig` → `Arc<dyn FileStorageDyn>`            |
| `kb` / `mb` / `gb`               | Size helper functions (return `usize` bytes)                             |
