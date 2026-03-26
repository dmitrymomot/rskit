# modo Conventions Reference

## File Organization

`mod.rs` and `lib.rs` are ONLY for `mod` imports and re-exports. All implementation code goes in separate files.

```
src/
  error/
    mod.rs          # mod core; mod convert; mod http_error; pub use ...
    core.rs         # Error struct, Result alias, constructors
    convert.rs      # From impls for std/third-party errors
    http_error.rs   # HttpError enum
  extractor/
    mod.rs          # mod json; mod form; ... pub use ...
    json.rs         # JsonRequest<T>
    form.rs         # FormRequest<T>
    query.rs        # Query<T>
    service.rs      # Service<T>
    multipart.rs    # MultipartRequest<T>, UploadedFile, Files
    upload_validator.rs  # UploadValidator
```

`lib.rs` re-exports key types at crate root for convenience:

```rust
pub use error::{Error, Result};
pub use extractor::Service;
pub use health::{HealthCheck, HealthChecks};
pub use sanitize::Sanitize;
pub use validate::{Validate, ValidationError, Validator};
```

External crates re-exported for user convenience:

```rust
pub use axum;
pub use serde;
pub use serde_json;
pub use sqlx;
pub use tokio;
```

---

## Error Handling

**Module:** `src/error/`
**Re-exports:** `modo::Error`, `modo::Result<T>`

### `Error`

```rust
pub struct Error {
    status: StatusCode,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    error_code: Option<&'static str>,
    details: Option<serde_json::Value>,
    lagged: bool,
}
```

**Constructors:**

```rust
// Generic
Error::new(status: StatusCode, message: impl Into<String>) -> Self
Error::with_source(status: StatusCode, message: impl Into<String>, source: impl Error + Send + Sync + 'static) -> Self

// Named status codes
Error::bad_request(msg: impl Into<String>) -> Self       // 400
Error::unauthorized(msg: impl Into<String>) -> Self      // 401
Error::forbidden(msg: impl Into<String>) -> Self         // 403
Error::not_found(msg: impl Into<String>) -> Self         // 404
Error::conflict(msg: impl Into<String>) -> Self          // 409
Error::payload_too_large(msg: impl Into<String>) -> Self // 413
Error::unprocessable_entity(msg: impl Into<String>) -> Self // 422
Error::too_many_requests(msg: impl Into<String>) -> Self // 429
Error::internal(msg: impl Into<String>) -> Self          // 500
Error::bad_gateway(msg: impl Into<String>) -> Self       // 502
Error::gateway_timeout(msg: impl Into<String>) -> Self   // 504
Error::lagged(skipped: u64) -> Self                      // 500, SSE-specific
```

**Builder methods:**

```rust
fn chain(self, source: impl Error + Send + Sync + 'static) -> Self  // attach source error
fn with_code(self, code: &'static str) -> Self                       // attach error identity code
fn with_details(self, details: serde_json::Value) -> Self            // attach structured JSON payload
```

**Accessors:**

```rust
fn status(&self) -> StatusCode
fn message(&self) -> &str
fn details(&self) -> Option<&serde_json::Value>
fn error_code(&self) -> Option<&str>
fn source_as<T: Error + 'static>(&self) -> Option<&T>  // downcast source
fn is_lagged(&self) -> bool
```

**`IntoResponse` output format:**

```json
{ "error": { "status": 404, "message": "user not found" } }
```

With details:

```json
{ "error": { "status": 422, "message": "validation failed", "details": { ... } } }
```

A copy of the error (without `source`) is stored in response extensions for middleware inspection.

**Usage pattern:**

```rust
use modo::{Error, Result};

async fn handler() -> Result<Json<User>> {
    let user = find_user(id)
        .map_err(|e| Error::not_found("user not found").chain(e))?;
    Ok(Json(user))
}
```

**Error identity pattern (for matching after response conversion):**

```rust
Error::unauthorized("unauthorized")
    .chain(JwtError::Expired)
    .with_code("jwt:expired")
// Before response: source_as::<JwtError>()
// After response:  error_code() == Some("jwt:expired")
```

### Gotchas

- `with_source(status, msg, source)` is a 3-arg constructor. The builder method is `chain(source)` (1 arg). Do not confuse them.
- `Clone` drops `source` (can't clone `Box<dyn Error>`). `error_code`, `details`, and all other fields are preserved.
- `IntoResponse` also drops `source`. Use `error_code` to preserve identity through the response pipeline.
- Adding fields to `Error` requires updating ALL struct literal sites (including `IntoResponse` and `Clone` impls).
- Guard/middleware errors must use `Error::into_response()` -- never construct raw HTTP responses.

### `HttpError`

Lightweight copy-able enum of common HTTP statuses. Converts into `Error` via `From<HttpError>`.

```rust
let err: Error = HttpError::NotFound.into();
assert_eq!(err.message(), "Not Found");
```

Variants: `BadRequest`, `Unauthorized`, `Forbidden`, `NotFound`, `MethodNotAllowed`, `Conflict`, `Gone`, `UnprocessableEntity`, `TooManyRequests`, `PayloadTooLarge`, `InternalServerError`, `BadGateway`, `ServiceUnavailable`, `GatewayTimeout`.

Methods:

- `fn status_code(self) -> StatusCode` -- returns the corresponding HTTP status code
- `fn message(self) -> &'static str` -- returns the canonical HTTP reason phrase

### Auto-conversions (`From` impls)

| Source type            | Maps to                                |
| ---------------------- | -------------------------------------- |
| `std::io::Error`       | 500 "IO error"                         |
| `serde_json::Error`    | 400 "JSON error"                       |
| `serde_yaml_ng::Error` | 500 "YAML error"                       |
| `ValidationError`      | 422 "validation failed" (with details) |

---

## Extractors

**Module:** `src/extractor/`

All extractors except `Path` and `Service<T>` require `T: Sanitize`. They call `sanitize()` automatically after deserialization.

### `Service<T>`

Retrieves a service from the `Registry`. Inner value is `Arc<T>`. Returns 500 if not registered.

```rust
pub struct Service<T>(pub Arc<T>);
// Bounds: T: Send + Sync + 'static
```

```rust
async fn handler(Service(db): Service<Pool>) {
    // db is Arc<Pool>
}
```

### `JsonRequest<T>`

Deserializes JSON body, then sanitizes.

```rust
pub struct JsonRequest<T>(pub T);
// Bounds: T: DeserializeOwned + Sanitize
// Rejection: 400 "invalid JSON: ..."
```

```rust
async fn create(JsonRequest(body): JsonRequest<CreateItem>) { ... }
```

### `FormRequest<T>`

Deserializes URL-encoded form body, then sanitizes.

```rust
pub struct FormRequest<T>(pub T);
// Bounds: T: DeserializeOwned + Sanitize
// Rejection: 400 "invalid form data: ..."
```

```rust
async fn login(FormRequest(form): FormRequest<LoginForm>) { ... }
```

### `Query<T>`

Deserializes URL query string, then sanitizes.

```rust
pub struct Query<T>(pub T);
// Bounds: T: DeserializeOwned + Sanitize
// Rejection: 400 "invalid query: ..."
```

```rust
async fn search(Query(params): Query<SearchParams>) { ... }
```

### `Path<T>`

Re-exported directly from `axum::extract::Path`. No sanitization.

```rust
pub use axum::extract::Path;
```

### `MultipartRequest<T>`

Splits `multipart/form-data` into text fields (deserialized + sanitized into `T`) and file fields (collected into `Files`).

```rust
pub struct MultipartRequest<T>(pub T, pub Files);
// Bounds: T: DeserializeOwned + Sanitize
// Rejection: 400 "invalid multipart request: ..."
```

```rust
async fn upload(MultipartRequest(form, mut files): MultipartRequest<ProfileForm>) {
    let avatar = files.file("avatar"); // Option<UploadedFile>
}
```

### `UploadedFile`

```rust
pub struct UploadedFile {
    pub name: String,          // original filename
    pub content_type: String,  // MIME type (default: "application/octet-stream")
    pub size: usize,           // bytes
    pub data: bytes::Bytes,    // raw file content
}

fn extension(&self) -> Option<String>           // lowercase, without dot
fn validate(&self) -> UploadValidator<'_>       // start fluent validation
async fn from_field(field: Field) -> Result<Self>  // low-level, prefer MultipartRequest
```

### `Files`

```rust
pub struct Files(HashMap<String, Vec<UploadedFile>>);

fn from_map(map: HashMap<String, Vec<UploadedFile>>) -> Self
fn get(&self, name: &str) -> Option<&UploadedFile>       // borrow first file
fn file(&mut self, name: &str) -> Option<UploadedFile>   // take first file
fn files(&mut self, name: &str) -> Vec<UploadedFile>     // take all files for field
```

### `UploadValidator`

Fluent validation for uploaded files. Obtained via `UploadedFile::validate()`.

```rust
fn max_size(self, max: usize) -> Self       // reject if file > max bytes
fn accept(self, pattern: &str) -> Self      // reject if content type doesn't match
fn check(self) -> Result<()>                // finalize; 422 error with all violations
```

Pattern formats: `"image/png"` (exact), `"image/*"` (wildcard subtype), `"*/*"` (any).

```rust
file.validate()
    .max_size(5 * 1024 * 1024)   // 5MB
    .accept("image/*")
    .check()?;
```

---

## Sanitize

**Module:** `src/sanitize/`
**Re-export:** `modo::Sanitize`

### `Sanitize` trait

```rust
pub trait Sanitize {
    fn sanitize(&mut self);
}
```

Called automatically by `JsonRequest`, `FormRequest`, `Query`, and `MultipartRequest` after deserialization.

```rust
use modo::sanitize::{Sanitize, trim, normalize_email};

#[derive(Deserialize)]
struct SignupInput {
    username: String,
    email: String,
}

impl Sanitize for SignupInput {
    fn sanitize(&mut self) {
        trim(&mut self.username);
        normalize_email(&mut self.email);
    }
}
```

### Helper functions

All operate in-place on `&mut String`:

```rust
fn trim(s: &mut String)                        // trim leading/trailing whitespace
fn trim_lowercase(s: &mut String)              // trim + lowercase
fn collapse_whitespace(s: &mut String)         // consecutive whitespace -> single space; preserves a single leading space (does NOT trim)
fn strip_html(s: &mut String)                  // remove tags, decode entities, strip script/style
fn truncate(s: &mut String, max_chars: usize)  // limit to N Unicode scalar values
fn normalize_email(s: &mut String)             // trim + lowercase + strip +tag
```

`normalize_email` example: `"  User+Tag@Example.COM  "` becomes `"user@example.com"`.

---

## Validate

**Module:** `src/validate/`
**Re-exports:** `modo::Validate`, `modo::ValidationError`, `modo::Validator`

### `Validate` trait

```rust
pub trait Validate {
    fn validate(&self) -> Result<(), ValidationError>;
}
```

### `Validator` (builder)

Implements `Default` (delegates to `new()`).

```rust
Validator::new()
    .field("name", &input.name, |f| f.required().min_length(2).max_length(100))
    .field("email", &input.email, |f| f.required().email())
    .field("age", &input.age, |f| f.range(18..=120))
    .check()   // -> Result<(), ValidationError>
```

All fields are validated (no short-circuit). Errors are collected per-field.

### `FieldValidator` rules

`FieldValidator` is an internal type (not re-exported). Users never name it directly -- it is the anonymous type received in the `Validator::field()` closure argument. Chain methods inside the closure.

**String rules** (available when `T: AsRef<str>`):

```rust
fn required(self) -> Self                       // non-empty after trim
fn min_length(self, min: usize) -> Self
fn max_length(self, max: usize) -> Self
fn email(self) -> Self                          // structural check: local@domain.tld
fn url(self) -> Self                            // starts with http(s)://, no spaces
fn one_of(self, options: &[&str]) -> Self
fn matches_regex(self, pattern: &str) -> Self
fn custom(self, predicate: impl FnOnce(&str) -> bool, message: &str) -> Self
```

**Numeric rules** (available when `T: PartialOrd + Display`):

```rust
fn range(self, range: RangeInclusive<T>) -> Self
```

### `ValidationError`

```rust
pub struct ValidationError {
    fields: HashMap<String, Vec<String>>,
}

fn new(fields: HashMap<String, Vec<String>>) -> Self
fn is_empty(&self) -> bool
fn field_errors(&self, field: &str) -> &[String]
fn fields(&self) -> &HashMap<String, Vec<String>>
```

Converts into `Error` automatically (HTTP 422) with the field map as `details`:

```rust
// In a handler:
input.validate()?;  // propagates as 422 with per-field errors
```

---

## Service Registry

**Module:** `src/service/`

### `Registry`

Mutable builder used at startup. Internally `HashMap<TypeId, Arc<dyn Any + Send + Sync>>`. Implements `Default` (delegates to `new()`).

```rust
fn new() -> Self
fn add<T: Send + Sync + 'static>(&mut self, service: T)   // register by type; replaces if exists
fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> // lookup for startup validation
fn into_state(self) -> AppState                            // freeze into immutable state
```

`Service<T>` extractor requires `AppState: FromRef<S>` on the router state type. This is automatic when using `Router::with_state(state)` where `state` is `AppState`, but custom composite state types must implement `FromRef`.

### `AppState`

Immutable, cheaply cloneable (wraps `Arc<HashMap<...>>`). Passed to `Router::with_state()`.

```rust
fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>>
```

### Startup flow

```rust
let mut registry = Registry::new();
registry.add(db_pool);
registry.add(email_client);

let state: AppState = registry.into_state();
let app = Router::new()
    .route("/", get(handler))
    .with_state(state);
```

In handlers, use `Service<T>` extractor:

```rust
async fn handler(Service(pool): Service<Pool>) {
    // pool is Arc<Pool>
}
```

---

## IDs

**Module:** `src/id/`
Always available, no feature flag.

### `id::ulid() -> String`

26-character ULID. Crockford base32, uppercase. 48-bit ms timestamp + 80-bit random.

```rust
let pk = modo::id::ulid();
// "01HQ3Y5KZXN9E4P7BVTG2WJMRS"  (26 chars)
```

Time-sortable: IDs generated later sort lexicographically after earlier ones.

### `id::short() -> String`

13-character base36 ID. Lowercase `0-9a-z`. 42-bit ms timestamp + 22-bit random.

```rust
let code = modo::id::short();
// "3f9kz7a2xnp01"  (13 chars)
```

Suitable for user-visible codes, slugs, short URLs.

### Gotchas

- No UUIDs anywhere. Use `ulid()` for primary keys, `short()` for user-facing codes.

---

## Encoding

**Module:** `src/encoding/`
Always available, no feature flag.

### `encoding::base32`

RFC 4648 base32, alphabet `A-Z 2-7`, no padding.

```rust
fn encode(bytes: &[u8]) -> String
fn decode(encoded: &str) -> modo::Result<Vec<u8>>
```

Decode is case-insensitive. Returns `Error::bad_request` on invalid characters.

```rust
use modo::encoding::base32;

let encoded = base32::encode(b"foobar");   // "MZXW6YTBOI"
let decoded = base32::decode("MZXW6YTBOI")?;  // b"foobar"
let decoded = base32::decode("mzxw6ytboi")?;  // also works
```

### `encoding::base64url`

RFC 4648 base64url, alphabet `A-Za-z0-9-_`, no padding. URL/cookie-safe.

```rust
fn encode(bytes: &[u8]) -> String
fn decode(encoded: &str) -> modo::Result<Vec<u8>>
```

Returns `Error::bad_request` on invalid characters.

```rust
use modo::encoding::base64url;

let encoded = base64url::encode(b"Hello");   // "SGVsbG8"
let decoded = base64url::decode("SGVsbG8")?; // b"Hello"
```

### `encoding::hex`

Lowercase hex encoding with a SHA-256 convenience helper.

```rust
fn encode(bytes: &[u8]) -> String
fn sha256(data: impl AsRef<[u8]>) -> String
```

```rust
use modo::encoding::hex;

let hex_str = hex::encode(b"\xde\xad");       // "dead"
let hash = hex::sha256(b"hello");              // 64-char lowercase hex
```

`encode` produces lowercase hex. `sha256` computes SHA-256 and returns the digest as lowercase hex (equivalent to `hex::encode(Sha256::digest(data))`).

### Gotchas

- These are modo's own implementations, NOT the `base64` crate. The `base64` crate is used separately for standard base64 in the webhooks feature.
- No padding characters are produced or accepted by the base32/base64url codecs.

---

## Cache

**Module:** `src/cache/`
Always available, no feature flag.

### `LruCache<K, V>`

Fixed-capacity least-recently-used cache. Backed by `HashMap` + `VecDeque`.

**Bounds:** `K: Eq + Hash + Clone`

```rust
fn new(capacity: NonZeroUsize) -> Self
fn get(&mut self, key: &K) -> Option<&V>    // moves key to most-recently-used
fn put(&mut self, key: K, value: V)         // inserts or updates; evicts LRU if full
```

```rust
use std::num::NonZeroUsize;
use modo::cache::LruCache;

let mut cache = LruCache::new(NonZeroUsize::new(100).unwrap());
cache.put("session_abc", session_data);

if let Some(data) = cache.get(&"session_abc") {
    // data is &SessionData; key moved to most-recently-used
}
```

### Gotchas

- `get` takes `&mut self` because it updates recency order. Even read-only lookups need exclusive access.
- NOT `Sync`. Wrap in `std::sync::RwLock` or `std::sync::Mutex` for multi-threaded use. Because `get` needs `&mut self`, even `RwLock` requires a write lock for reads.
- O(n) recency update (linear scan of VecDeque). Fine for caches up to a few thousand entries.
- Use `std::sync::RwLock` (not tokio) for all sync-only state -- never hold across `.await`.

---

## Health Checks

**Module:** `src/health/`
Always available, no feature flag.
**Re-exports:** `modo::HealthCheck`, `modo::HealthChecks`

Provides liveness and readiness probe endpoints for Kubernetes-style health checks.

### Endpoints

- `/_live` -- always returns 200 OK (liveness probe)
- `/_ready` -- runs all registered checks concurrently, returns 200 if all pass, 503 if any fail; failures logged at ERROR level

### `HealthCheck` trait

```rust
pub trait HealthCheck: Send + Sync + 'static {
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}
```

Built-in implementations:

- `db::Pool` -- acquires a connection to verify pool health
- `db::ReadPool` -- acquires a connection to verify read pool health
- `db::WritePool` -- acquires a connection to verify write pool health

### `HealthChecks`

A collection of named health checks. Built with a fluent API. Implements `Default` (delegates to `new()`).

```rust
fn new() -> Self
fn check(self, name: &str, c: impl HealthCheck) -> Self     // register trait impl
fn check_fn<F, Fut>(self, name: &str, f: F) -> Self          // register closure
```

`check_fn` bounds: `F: Fn() -> Fut + Send + Sync + 'static`, `Fut: Future<Output = Result<()>> + Send + 'static`.

### `router()`

```rust
pub fn router() -> Router<AppState>
```

Returns a router with `/_live` and `/_ready` routes. Merge into your app router.

### Usage

```rust
use modo::health::{HealthChecks, router};
use modo::service::Registry;

let checks = HealthChecks::new()
    .check("read_pool", read_pool.clone())
    .check("write_pool", write_pool.clone())
    .check_fn("redis", || async { Ok(()) });

let mut registry = Registry::new();
registry.add(checks);

let app = axum::Router::new()
    .merge(router())
    .with_state(registry.into_state());
```

### Gotchas

- `HealthChecks` must be registered in the `Registry` before the readiness endpoint works. If not registered, `Service<HealthChecks>` returns 500.
- `HealthCheck` uses `Pin<Box<dyn Future>>` returns (not RPITIT) to stay object-safe behind `Arc<dyn HealthCheck>`.
- All checks run concurrently via `JoinSet`. A panicking check is treated as a failure (503).
