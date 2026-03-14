# File Upload Reference

The `modo-upload` crate provides multipart form parsing, in-memory file buffering, per-field
validation, and pluggable storage backends. The `#[derive(FromMultipart)]` macro (from
`modo-upload-macros`) generates the `FromMultipart` implementation. The `MultipartForm<T>`
extractor integrates with axum's request extraction pipeline and reads `UploadConfig` from the
app's service registry.

---

## Documentation

- modo-upload crate: https://docs.rs/modo-upload
- modo-upload-macros crate: https://docs.rs/modo-upload-macros

---

## Multipart Parsing

### `#[derive(FromMultipart)]`

Apply the derive macro to any struct with named fields. The macro generates an implementation of
the `FromMultipart` trait, which `MultipartForm<T>` calls automatically during request extraction.
Only structs with named fields are supported — tuple structs and enums are a compile error.

```rust
use modo_upload::{FromMultipart, UploadedFile};

#[derive(FromMultipart)]
struct ProfileForm {
    #[upload(max_size = "5mb", accept = "image/*")]
    avatar: UploadedFile,

    name: String,

    #[upload(min_count = 1, max_count = 5)]
    attachments: Vec<UploadedFile>,

    #[serde(rename = "user_email")]
    email: Option<String>,
}
```

### Supported Field Types

| Rust type              | Behaviour                                           |
|------------------------|-----------------------------------------------------|
| `UploadedFile`         | Required file field; errors if absent               |
| `Option<UploadedFile>` | Optional file field                                 |
| `Vec<UploadedFile>`    | Multiple files under the same field name            |
| `BufferedUpload`       | Required streaming upload; at most one per struct   |
| `String`               | Required text field                                 |
| `Option<String>`       | Optional text field                                 |
| any `T: FromStr`       | Required text field, parsed via `FromStr`           |

### Field Name Override

By default the Rust field name is used as the multipart field name. Use `#[serde(rename = "...")]`
to override it:

```rust
#[derive(FromMultipart)]
struct ContactForm {
    #[serde(rename = "full_name")]
    name: String,
}
```

### `MultipartForm<T>` Extractor

`MultipartForm<T>` is an axum extractor. It reads `UploadConfig` from the service registry to
apply the global `max_file_size` limit. Auto-sanitization (`modo::sanitize::auto_sanitize`) runs
on the parsed struct before it is returned. When `T` also implements `modo::validate::Validate`,
call `.validate()` on the wrapper to run additional field-level rules.

```rust
use modo::JsonResult;
use modo::Service;
use modo_upload::{FileStorage, MultipartForm};

use crate::types::ProfileForm;

#[modo::handler(POST, "/profile")]
async fn update_profile(
    storage: Service<Arc<dyn FileStorage>>,
    form: MultipartForm<ProfileForm>,
) -> JsonResult<serde_json::Value> {
    form.validate()?;
    let stored = storage.store("avatars", &form.avatar).await?;
    Ok(modo::Json(serde_json::json!({
        "name": form.name,
        "avatar_path": stored.path,
    })))
}
```

`MultipartForm<T>` implements `Deref<Target = T>`, so fields are accessible directly via `form.field`.
Use `.into_inner()` to consume the wrapper and take ownership of the inner `T`.

### `UploadedFile`

An uploaded file fully buffered in memory. Available accessors:

| Method              | Return type      | Description                                  |
|---------------------|------------------|----------------------------------------------|
| `name()`            | `&str`           | Multipart field name                         |
| `file_name()`       | `&str`           | Original filename from the client            |
| `content_type()`    | `&str`           | MIME content type                            |
| `data()`            | `&bytes::Bytes`  | Raw file bytes                               |
| `size()`            | `usize`          | File size in bytes                           |
| `extension()`       | `Option<String>` | File extension from the original name (lowercase, without dot) |
| `is_empty()`        | `bool`           | Whether the file has zero bytes              |
| `validate()`        | `UploadValidator`| Start a fluent validation chain              |

### `BufferedUpload`

A chunked upload buffered in memory as a `Vec<Bytes>`. Useful when you need streaming I/O after
parsing, for example when writing directly to storage without a full-copy allocation. Only one
`BufferedUpload` field is allowed per struct.

| Method          | Return type               | Description                            |
|-----------------|---------------------------|----------------------------------------|
| `name()`        | `&str`                    | Multipart field name                   |
| `file_name()`   | `&str`                    | Original filename                      |
| `content_type()`| `&str`                    | MIME content type                      |
| `size()`        | `usize`                   | Total bytes across all chunks          |
| `chunk()`       | `Option<Result<Bytes, _>>`| Read next chunk (consumes sequentially)|
| `into_reader()` | `Pin<Box<dyn AsyncRead>>` | Convert to a tokio `AsyncRead`         |
| `to_bytes()`    | `bytes::Bytes`            | Collapse all chunks into one `Bytes`   |

---

## Field Validation

### Declare-time validation via `#[upload(...)]`

Validation attributes are evaluated at parse time, before the handler is called. A validation
failure returns a structured error with per-field messages.

| Attribute              | Applies to                    | Description                                   |
|------------------------|-------------------------------|-----------------------------------------------|
| `max_size = "<size>"`  | `UploadedFile`, `Vec<UploadedFile>` | Maximum file size. Accepts `"5mb"`, `"100kb"`, `"2gb"`, or plain bytes. Case-insensitive. |
| `accept = "<pattern>"` | `UploadedFile`, `Vec<UploadedFile>` | MIME type pattern. Supports exact types (`"application/pdf"`) and wildcards (`"image/*"`, `"*/*"`). |
| `min_count = <n>`      | `Vec<UploadedFile>`           | Minimum number of files.                      |
| `max_count = <n>`      | `Vec<UploadedFile>`           | Maximum number of files.                      |

```rust
#[derive(FromMultipart)]
struct DocumentForm {
    // Exactly one PDF, max 10 MB
    #[upload(max_size = "10mb", accept = "application/pdf")]
    document: UploadedFile,

    // One to three images
    #[upload(min_count = 1, max_count = 3, accept = "image/*")]
    photos: Vec<UploadedFile>,

    // Optional thumbnail — only validated when present
    #[upload(max_size = "500kb", accept = "image/*")]
    thumbnail: Option<UploadedFile>,
}
```

### Runtime validation via `UploadValidator`

For validations that cannot be expressed at derive time, call `.validate()` on any `UploadedFile`
to get a fluent `UploadValidator`:

```rust
file.validate()
    .max_size(mb(5))    // uses the mb() helper
    .accept("image/*")
    .check()?;
```

The validator chains calls and collects all failures before returning a single structured error.
Available methods:

| Method             | Description                                          |
|--------------------|------------------------------------------------------|
| `.max_size(usize)` | Reject if file size exceeds the given byte count.    |
| `.accept(&str)`    | Reject if MIME type does not match the pattern.      |
| `.check()`         | Finalise. Returns `Ok(())` or a validation error.    |

### Size helper functions

Three free functions convert human-readable sizes to bytes:

```rust
use modo_upload::{kb, mb, gb};

let limit = mb(5);   // 5 * 1024 * 1024
let small = kb(100); // 100 * 1024
let large = gb(1);   // 1 * 1024 * 1024 * 1024
```

### MIME matching rules

MIME patterns follow these rules (implemented in `mime_matches`):

- `"*/*"` matches any content type.
- `"image/*"` matches any type whose main type is `image`.
- `"image/png"` matches only `image/png`.
- Parameters after `;` (e.g. `image/png; charset=utf-8`) are stripped before matching.
- Matching is case-sensitive — `"Image/PNG"` does not match `"image/png"`.

### Global size limit from config

`MultipartForm<T>` reads `UploadConfig::max_file_size` from the service registry and passes it
as the global `max_file_size` to every file field. Per-field `#[upload(max_size = ...)]` is
checked after collection and takes precedence in the validation error message, but the global
limit is enforced during streaming to prevent over-read.

---

## Storage Backends

### `UploadConfig`

Configure uploads in the app's YAML config, deserialized via `modo::config::load()`:

```yaml
upload:
  backend: local       # "local" (default) or "s3"
  path: ./uploads      # base directory for local storage
  max_file_size: 10mb  # global default size limit

  # Only relevant when backend = "s3" (requires opendal feature)
  s3:
    bucket: my-bucket
    region: us-east-1
    endpoint: ""        # leave empty for AWS; set for MinIO etc.
    access_key_id: ""
    secret_access_key: ""
```

In Rust, embed `UploadConfig` into your app's config struct:

```rust
use modo_upload::UploadConfig;

#[derive(Default, Deserialize)]
struct Config {
    #[serde(flatten)]
    core: modo::config::AppConfig,
    #[serde(default)]
    upload: UploadConfig,
}
```

`UploadConfig` defaults: `backend = Local`, `path = "./uploads"`, `max_file_size = Some("10mb")`.

### `StorageBackend` enum

```rust
pub enum StorageBackend {
    Local,  // default
    S3,     // requires opendal feature
}
```

Serializes/deserializes as lowercase strings (`"local"`, `"s3"`).

### `storage()` factory function

Construct a `Arc<dyn FileStorage>` from config, then register it as a service:

```rust
#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = modo_upload::storage(&config.upload)?;
    app.config(config.core).service(storage).run().await
}
```

After registration, inject it into handlers via `Service<Arc<dyn FileStorage>>`.

### `FileStorage` trait

```rust
#[async_trait]
pub trait FileStorage: Send + Sync + 'static {
    async fn store(&self, prefix: &str, file: &UploadedFile)
        -> Result<StoredFile, modo::Error>;

    async fn store_stream(&self, prefix: &str, stream: &mut BufferedUpload)
        -> Result<StoredFile, modo::Error>;

    async fn delete(&self, path: &str) -> Result<(), modo::Error>;

    async fn exists(&self, path: &str) -> Result<bool, modo::Error>;
}
```

All methods take a `prefix` (e.g. `"avatars"`) and return a `StoredFile` with `path` and `size`
fields. The path is relative within the storage root — store it in the database, not the full
absolute path.

### `StoredFile`

```rust
pub struct StoredFile {
    pub path: String,  // relative path, e.g. "avatars/01hxk3q1a2b3.jpg"
    pub size: u64,
}
```

Filenames are ULID-based (`{ulid}.{ext}`), generated automatically. The original filename is
never used in the stored path to avoid injection and collision issues.

### Local filesystem storage (`LocalStorage`)

Requires the `local` feature (enabled by default). Files are written to
`<base_dir>/<prefix>/<ulid>.<ext>`. The directory is created automatically on the first store
call.

```rust
use modo_upload::storage::local::LocalStorage;

let storage = LocalStorage::new("./uploads");
```

Path traversal protection: `..` components and absolute paths in the prefix or generated path
are rejected before any filesystem operation. The storage function factory builds a
`LocalStorage` automatically when `backend = local`.

### S3 / OpenDAL storage (`OpendalStorage`)

Requires the `opendal` feature. Wraps any Apache OpenDAL `Operator`. The `storage()` factory
builds the S3 operator from `S3Config` automatically:

```rust
use modo_upload::storage::opendal::OpendalStorage;
use opendal::{Operator, services::S3};

let op = Operator::new(
    S3::default()
        .bucket("my-bucket")
        .region("us-east-1")
        .access_key_id("key")
        .secret_access_key("secret"),
)?.finish();

let storage = OpendalStorage::new(op);
```

For S3-compatible services (MinIO, Cloudflare R2, Tigris), set `endpoint` in `S3Config` or pass
it to the builder via `.endpoint(url)`.

Logical path safety: object-store keys are validated before every operation — leading `/`,
`.`, and `..` segments are rejected to prevent path confusion.

---

## Integration Patterns

### Full upload handler with validation

The upload example shows the complete pattern combining `#[derive(FromMultipart)]`, `MultipartForm`,
`Service<Arc<dyn FileStorage>>`, and runtime validation:

```rust
use modo::JsonResult;
use modo::Service;
use modo_upload::{FileStorage, MultipartForm, UploadedFile, FromMultipart};

#[derive(FromMultipart, modo::Sanitize, modo::Validate)]
pub struct ProfileForm {
    #[upload(max_size = "5mb", accept = "image/*")]
    pub avatar: UploadedFile,

    #[clean(trim)]
    #[validate(required, min_length = 2)]
    pub name: String,

    #[clean(trim, normalize_email)]
    #[validate(required, email)]
    pub email: String,
}

#[modo::handler(POST, "/profile")]
async fn update_profile(
    storage: Service<Arc<dyn FileStorage>>,
    form: MultipartForm<ProfileForm>,
) -> JsonResult<serde_json::Value> {
    form.validate()?;
    let stored = storage.store("avatars", &form.avatar).await?;
    Ok(modo::Json(serde_json::json!({
        "name": form.name,
        "avatar_path": stored.path,
    })))
}
```

Key observations:
- `storage` is extracted before `form` — extractor order in the function signature matches
  extraction order.
- `form.validate()?` calls the `modo::Validate` rules (e.g. `required`, `min_length`).
  The `#[upload(...)]` rules are enforced before this, during multipart parsing.
- `storage.store("avatars", &form.avatar)` returns `StoredFile` with the relative path.

### Upload with authentication middleware

When routes require authentication, attach auth middleware at the module or handler level.
Middleware stacking order is outermost-first: Global → Module → Handler. Auth middleware runs
_before_ `MultipartForm` extraction, which is correct — you never want to parse the body for an
unauthenticated request.

```rust
// In your router setup
let module = modo::Module::new()
    .middleware(auth_required_middleware)   // runs first — rejects unauthed before body is read
    .handler(update_profile);
```

Avoid placing auth middleware _after_ extractors are evaluated. Because axum resolves extractors
in function parameter order, `MultipartForm` will buffer the entire body before middleware can
reject the request if middleware is applied at the wrong layer.

### Multiple file fields

```rust
#[derive(FromMultipart)]
struct ReportForm {
    title: String,

    // Single required cover image
    #[upload(max_size = "2mb", accept = "image/*")]
    cover: UploadedFile,

    // 1–10 attachments of any type
    #[upload(min_count = 1, max_count = 10, max_size = "20mb")]
    attachments: Vec<UploadedFile>,

    // Optional supplemental document
    #[upload(accept = "application/pdf")]
    supplement: Option<UploadedFile>,
}
```

### Storing a `BufferedUpload`

Use `BufferedUpload` when you need to process the file as a stream after parsing, or pass it
directly to `store_stream`:

```rust
#[derive(FromMultipart)]
struct VideoForm {
    title: String,
    video: BufferedUpload,
}

#[modo::handler(POST, "/video")]
async fn upload_video(
    storage: Service<Arc<dyn FileStorage>>,
    form: MultipartForm<VideoForm>,
) -> JsonResult<serde_json::Value> {
    let mut form = form.into_inner();
    let stored = storage.store_stream("videos", &mut form.video).await?;
    Ok(modo::Json(serde_json::json!({ "path": stored.path })))
}
```

### Delete and existence check

```rust
// Check before deleting to give a meaningful error
if storage.exists(&old_path).await? {
    storage.delete(&old_path).await?;
}
```

---

## Gotchas

- **`MultipartForm` consumes the request body.** It must appear only once as an extractor. Do not
  also use `axum::extract::Multipart` in the same handler.

- **Extractor order matters for auth.** Place auth extractors or middleware before `MultipartForm`
  in the parameter list (or at a higher middleware layer). If auth fails, the body should not be
  buffered at all.

- **Global size limit vs. per-field limit.** The global `UploadConfig::max_file_size` is enforced
  during streaming (inside `UploadedFile::from_field` / `BufferedUpload::from_field`) to stop
  over-read early. Per-field `#[upload(max_size = ...)]` is checked after collection and emits a
  validation-style error keyed to the field name. Set the global limit conservatively and use
  per-field limits for finer-grained messages.

- **`BufferedUpload` limit: one per struct.** Having more than one `BufferedUpload` field on the
  same struct is a compile error from the derive macro.

- **MIME matching is case-sensitive.** Client-supplied content types like `Image/PNG` will not
  match `image/*`. Normalise on the client side or accept the occasional false-negative.

- **Filename uniqueness.** Stored filenames are ULID-based; original filenames are never used in
  storage paths. Do not attempt to reconstruct the original filename from the stored path — store
  it separately in the database if needed.

- **`opendal` feature gate.** The `S3Config` struct and `OpendalStorage` type are only compiled
  when the `opendal` feature is enabled. The `StorageBackend::S3` variant always exists in code,
  but calling `storage()` with `S3` backend without the feature returns a runtime error.

- **Local directory creation.** `LocalStorage` creates the target directory on the first write.
  If the process does not have write permission to the parent, the first upload will fail at
  runtime, not at startup.

- **`service(storage)` registers `Arc<dyn FileStorage>`.** Inject it exactly as
  `Service<Arc<dyn FileStorage>>` in handlers. Using `Service<LocalStorage>` or
  `Service<OpendalStorage>` directly will fail to resolve.

- **Auto-sanitization.** `MultipartForm` calls `modo::sanitize::auto_sanitize` on the parsed
  struct automatically. Fields annotated with `#[clean(trim)]` are trimmed before `validate()`
  runs, so you do not need to trim manually.

---

## docs.rs Links

| Type / Item              | URL                                                            |
|--------------------------|----------------------------------------------------------------|
| `FromMultipart` trait    | https://docs.rs/modo-upload/latest/modo_upload/trait.FromMultipart.html |
| `FromMultipart` derive   | https://docs.rs/modo-upload-macros/latest/modo_upload_macros/derive.FromMultipart.html |
| `MultipartForm<T>`       | https://docs.rs/modo-upload/latest/modo_upload/struct.MultipartForm.html |
| `UploadedFile`           | https://docs.rs/modo-upload/latest/modo_upload/struct.UploadedFile.html |
| `BufferedUpload`         | https://docs.rs/modo-upload/latest/modo_upload/struct.BufferedUpload.html |
| `FileStorage` trait      | https://docs.rs/modo-upload/latest/modo_upload/storage/trait.FileStorage.html |
| `StoredFile`             | https://docs.rs/modo-upload/latest/modo_upload/storage/struct.StoredFile.html |
| `StorageBackend`         | https://docs.rs/modo-upload/latest/modo_upload/enum.StorageBackend.html |
| `UploadConfig`           | https://docs.rs/modo-upload/latest/modo_upload/struct.UploadConfig.html |
| `S3Config`               | https://docs.rs/modo-upload/latest/modo_upload/struct.S3Config.html |
| `LocalStorage`           | https://docs.rs/modo-upload/latest/modo_upload/storage/local/struct.LocalStorage.html |
| `OpendalStorage`         | https://docs.rs/modo-upload/latest/modo_upload/storage/opendal/struct.OpendalStorage.html |
| `UploadValidator`        | https://docs.rs/modo-upload/latest/modo_upload/struct.UploadValidator.html |
| `storage()` function     | https://docs.rs/modo-upload/latest/modo_upload/storage/fn.storage.html |
| `kb()`, `mb()`, `gb()`   | https://docs.rs/modo-upload/latest/modo_upload/fn.mb.html |
