# modo::extractor

Request extractors for the modo web framework.

All sanitizing extractors call [`Sanitize::sanitize`](../sanitize/index.html) on the
deserialized value before returning it, so whitespace trimming and other normalization
happen automatically.

## Key Types

| Type                  | Source                               | Trait bound on `T`            |
| --------------------- | ------------------------------------ | ----------------------------- |
| `JsonRequest<T>`      | JSON request body                    | `DeserializeOwned + Sanitize` |
| `FormRequest<T>`      | URL-encoded form body                | `DeserializeOwned + Sanitize` |
| `Query<T>`            | URL query string                     | `DeserializeOwned + Sanitize` |
| `MultipartRequest<T>` | `multipart/form-data` body           | `DeserializeOwned + Sanitize` |
| `Path<T>`             | URL path parameters (axum re-export) | `DeserializeOwned`            |
| `Service<T>`          | Application service registry         | `Send + Sync + 'static`       |
| `UploadedFile`        | Single file from a multipart upload  | —                             |
| `Files`               | Map of field names to uploaded files | —                             |
| `UploadValidator<'_>` | Fluent file validator                | —                             |
| `ClientInfo`          | Client IP, user-agent, fingerprint   | —                             |

## Usage

### JSON body

```rust
use modo::extractor::JsonRequest;
use modo::Sanitize;
use serde::Deserialize;

#[derive(Deserialize)]
struct CreateItem {
    name: String,
}

impl Sanitize for CreateItem {
    fn sanitize(&mut self) {
        self.name = self.name.trim().to_string();
    }
}

async fn create(JsonRequest(body): JsonRequest<CreateItem>) {
    // body.name is already trimmed
}
```

### URL-encoded form

```rust
use modo::extractor::FormRequest;
use modo::Sanitize;
use serde::Deserialize;

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

impl Sanitize for LoginForm {
    fn sanitize(&mut self) {
        self.username = self.username.trim().to_lowercase();
    }
}

async fn login(FormRequest(form): FormRequest<LoginForm>) {
    // form.username is trimmed and lowercased
}
```

### Query string

```rust
use modo::extractor::Query;
use modo::Sanitize;
use serde::Deserialize;

#[derive(Deserialize)]
struct SearchParams {
    q: String,
    page: Option<u32>,
}

impl Sanitize for SearchParams {
    fn sanitize(&mut self) {
        self.q = self.q.trim().to_lowercase();
    }
}

async fn search(Query(params): Query<SearchParams>) {
    // params.q is trimmed and lowercased
}
```

### Multipart file upload

`MultipartRequest<T>` deconstructs into `(T, Files)`. Text fields are deserialized and
sanitized into `T`; file fields are accessible through the [`Files`] map.

```rust
use modo::extractor::{MultipartRequest, Files};
use modo::Sanitize;
use modo::Result;
use serde::Deserialize;

#[derive(Deserialize)]
struct ProfileForm {
    display_name: String,
}

impl Sanitize for ProfileForm {
    fn sanitize(&mut self) {
        self.display_name = self.display_name.trim().to_string();
    }
}

async fn update_profile(
    MultipartRequest(form, mut files): MultipartRequest<ProfileForm>,
) -> Result<()> {
    if let Some(avatar) = files.file("avatar") {
        // avatar.name, avatar.content_type, avatar.size, avatar.data
        avatar.validate()
            .max_size(5 * 1024 * 1024)  // 5 MB
            .accept("image/*")
            .check()?;
    }
    Ok(())
}
```

### Service registry

`Service<T>` retrieves a service registered via `Registry::add` during startup.
The inner value is `Arc<T>`. Returns 500 if `T` was not registered.

```rust
use modo::service::Service;
use std::sync::Arc;

struct EmailService { /* ... */ }

async fn handler(Service(email): Service<EmailService>) {
    // email is Arc<EmailService>
}
```

### Client info

`ClientInfo` extracts the client IP address, `User-Agent` header, and
`X-Fingerprint` header from the request. Requires `ClientIpLayer` for the IP
field; without it, `ip_value()` returns `None`.

For non-HTTP contexts (background jobs, CLI tools), use the builder:

```rust
use modo::ip::ClientInfo;

let info = ClientInfo::new()
    .ip("1.2.3.4")
    .user_agent("my-script/1.0");

assert_eq!(info.ip_value(), Some("1.2.3.4"));
```

In a handler:

```rust,ignore
use modo::ip::ClientInfo;

async fn handler(client: ClientInfo) {
    if let Some(ip) = client.ip_value() {
        // ...
    }
}
```

### File validation

`UploadedFile::validate()` returns an `UploadValidator` for fluent constraint checking.
All violations are collected before the error is returned (422 Unprocessable Entity).

Supported patterns for `.accept`: exact (`"image/png"`), wildcard subtype (`"image/*"`),
and catch-all (`"*/*"`). Parameters after `;` in the content type are stripped before
matching.

```rust
use modo::extractor::UploadedFile;

fn check_avatar(file: &UploadedFile) -> modo::Result<()> {
    file.validate()
        .max_size(2 * 1024 * 1024)  // 2 MB
        .accept("image/*")          // allow any image type
        .check()
}
```
