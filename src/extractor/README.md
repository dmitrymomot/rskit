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
| `UploadedFile`        | Single file from a multipart upload  | —                             |
| `Files`               | Map of field names to uploaded files | —                             |
| `UploadValidator<'_>` | Fluent file validator                | —                             |

## Usage

### JSON body

```rust
use modo::extractor::JsonRequest;
use modo::sanitize::Sanitize;
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
use modo::sanitize::Sanitize;
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
use modo::sanitize::Sanitize;
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

### Path parameters

`Path<T>` is re-exported from axum unchanged; it does not sanitize.

```rust
use modo::extractor::Path;

async fn show_item(Path(id): Path<String>) {
    // id is the matched path segment
}
```

### Multipart file upload

`MultipartRequest<T>` deconstructs into `(T, Files)`. Text fields are deserialized and
sanitized into `T`; file fields are accessible through the [`Files`] map.

```rust
use modo::extractor::{MultipartRequest, Files};
use modo::sanitize::Sanitize;
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

`Files` exposes three accessors for a given field name:

- `files.get("field")` — shared `Option<&UploadedFile>` reference to the first file.
- `files.file("field")` — takes ownership of the first `UploadedFile`.
- `files.files("field")` — takes ownership of all `UploadedFile` values.

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

`UploadedFile::extension()` returns the lowercased extension without the leading dot
(e.g. `Some("jpg")` for `"photo.JPG"`), or `None` if the filename has no extension.
