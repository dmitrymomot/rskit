# modo::extractor

Sanitizing axum extractors for request bodies, query strings, and multipart uploads
(modo 0.10).

All sanitizing extractors call [`Sanitize::sanitize`](../sanitize/index.html) on the
deserialized value before returning it, so whitespace trimming and other normalization
happen automatically. Every rejection is a `modo::Error` that renders through
`Error::into_response()` — handlers never see raw HTTP responses.

## Extractors

| Type                  | Extracts                               | Trait impl          | Rejection (`modo::Error`)                                                                                |
| --------------------- | -------------------------------------- | ------------------- | -------------------------------------------------------------------------------------------------------- |
| `JsonRequest<T>`      | JSON request body                      | `FromRequest`       | `400 Bad Request` — missing/invalid content-type, malformed JSON, deserialization into `T` failed        |
| `FormRequest<T>`      | URL-encoded form body                  | `FromRequest`       | `400 Bad Request` — body is not valid `application/x-www-form-urlencoded` or cannot deserialize into `T` |
| `Query<T>`            | URL query string                       | `FromRequestParts`  | `400 Bad Request` — query string cannot deserialize into `T`                                             |
| `MultipartRequest<T>` | `multipart/form-data` body → `(T, Files)` | `FromRequest`    | `400 Bad Request` — not valid multipart, a field cannot be read, or text fields cannot deserialize into `T` |
| `Path<T>`             | URL path parameters (axum re-export)   | `FromRequestParts`  | axum's `PathRejection` — no sanitization                                                                 |
| `UploadedFile`        | Single file from a multipart field     | value type          | —                                                                                                        |
| `Files`               | Map of field names to uploaded files   | value type          | —                                                                                                        |
| `UploadValidator<'_>` | Fluent size/content-type validator     | value type          | `.check()` returns `422 Unprocessable Entity` with a `details` payload listing violations                |

`T: DeserializeOwned + Sanitize` applies to every extractor except `Path<T>` (only
`DeserializeOwned`).

### Optional extractors

In axum 0.8, `Option<MyExtractor>` requires an explicit `OptionalFromRequestParts`
impl — none is provided for the modo extractors above. Instead, make the wrapped
type tolerant: use `#[serde(default)]` or `Option<_>` fields on the deserialized
struct (e.g. `SearchParams { q: Option<String>, page: Option<u32> }`).

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

`FormRequest` reads the body through `axum::body::Bytes`, so any
`DefaultBodyLimit` (or `RequestBodyLimit` middleware) you apply to the router is
honored — oversized requests short-circuit with `413 Payload Too Large` before any
deserialization runs.

#### Repeated keys (multi-select checkboxes / dropdowns)

`FormRequest`, `Query`, and the text-field side of `MultipartRequest` deserialize via
`serde_qs`, so a repeated form key populates a `Vec<…>` field. A 7-checkbox work-day
picker that posts `work_days=1&work_days=2&work_days=3&work_days=4&work_days=5`
arrives as `Vec<u8>` with five elements:

```rust
use modo::extractor::FormRequest;
use modo::sanitize::Sanitize;
use serde::Deserialize;

#[derive(Deserialize)]
struct NewEmployee {
    name: String,
    work_days: Vec<u8>,        // multi-select checkbox group
    policy_ids: Vec<String>,   // multi-select list
}

impl Sanitize for NewEmployee {
    fn sanitize(&mut self) {
        self.name = self.name.trim().to_string();
    }
}

async fn create(FormRequest(form): FormRequest<NewEmployee>) {
    // form.work_days = [1, 2, 3, 4, 5]
}
```

#### Nested structs (bracketed keys)

For grouped fields, use bracket notation in the input names:

```html
<input name="address[city]" />
<input name="address[postcode]" />
```

```rust
#[derive(Deserialize)]
struct Address { city: String, postcode: String }

#[derive(Deserialize)]
struct OrderForm { customer_email: String, address: Address }

impl Sanitize for OrderForm {
    fn sanitize(&mut self) {
        self.customer_email = self.customer_email.trim().to_lowercase();
    }
}

async fn place_order(FormRequest(o): FormRequest<OrderForm>) {
    // o.address.city, o.address.postcode
}
```

#### `Vec<Struct>` for dynamic rows (htmx, JS-added rows)

For per-row dynamic forms — e.g. a contact list where each row has `kind`, `value`,
and `comment` — use **indexed** bracket notation. The index disambiguates which
fields belong to which row:

```html
<!-- Row 0 -->
<input name="contacts[0][kind]"    value="email" />
<input name="contacts[0][value]"   value="a@b.com" />
<input name="contacts[0][comment]" value="primary" />
<!-- Row 1 -->
<input name="contacts[1][kind]"    value="phone" />
<input name="contacts[1][value]"   value="555-0100" />
<input name="contacts[1][comment]" value="" />
```

```rust
#[derive(Deserialize)]
struct Contact { kind: String, value: String, comment: String }

#[derive(Deserialize)]
struct NewClient { name: String, contacts: Vec<Contact> }

impl Sanitize for NewClient {
    fn sanitize(&mut self) { self.name = self.name.trim().to_string(); }
}

async fn save(FormRequest(c): FormRequest<NewClient>) {
    // c.contacts is one entry per submitted row, in index order
}
```

> **Indexed names are required for `Vec<Struct>`.** Without indices the
> deserializer cannot tell which fields belong to which row. For top-level
> `Vec<scalar>` fields (e.g. `tag=a&tag=b`) the indices are optional.

#### Files alongside nested fields (multipart)

`MultipartRequest<T>` returns `(T, Files)`. Nested-struct deserialization applies to
the text fields in `T`; uploaded files come back through the `Files` map keyed by
the multipart field name (which can itself use bracket notation):

```rust
use modo::extractor::{MultipartRequest, Files};

# use serde::Deserialize;
# use modo::sanitize::Sanitize;
#[derive(Deserialize)]
struct Contact { kind: String, value: String }
#[derive(Deserialize)]
struct NewClient { name: String, contacts: Vec<Contact> }
impl Sanitize for NewClient { fn sanitize(&mut self) {} }

async fn save(MultipartRequest(c, mut files): MultipartRequest<NewClient>) {
    let avatar = files.file("avatar"); // Option<UploadedFile>
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

#### Nested filters and percent-encoded brackets

`Query<T>` uses `serde_qs` form-encoding mode, so it accepts both bare brackets and
the percent-encoded form most browsers emit when JS builds the URL with
`URLSearchParams`. Both of these decode into the same `Filter` struct:

```text
GET /clients?filter[status]=active&filter[role]=admin
GET /clients?filter%5Bstatus%5D=active&filter%5Brole%5D=admin
```

```rust
use modo::extractor::Query;
use modo::sanitize::Sanitize;
use serde::Deserialize;

#[derive(Deserialize)]
struct Filter { status: String, role: String }

#[derive(Deserialize)]
struct ListClients {
    page: Option<u32>,
    filter: Filter,           // filter[status]=…&filter[role]=…
    tags: Vec<String>,        // tags=a&tags=b
}

impl Sanitize for ListClients {
    fn sanitize(&mut self) {}
}

async fn list_clients(Query(p): Query<ListClients>) {
    // p.filter.status == "active" for either bracket encoding above
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

## Advanced use

### Building `Files` from a pre-existing map

`Files::from_map` lets you construct a [`Files`] collection from a
`HashMap<String, Vec<UploadedFile>>` that you assembled yourself — useful in tests or
when pre-processing fields before passing them to application logic.

```rust,no_run
use std::collections::HashMap;
use modo::extractor::{Files, UploadedFile};

let map: HashMap<String, Vec<UploadedFile>> = HashMap::new();
let files = Files::from_map(map);
```

### Reading a single multipart field manually

`UploadedFile::from_field` consumes an `axum_extra` multipart field and reads it fully
into memory. Prefer [`MultipartRequest`] for ordinary handlers; use this only when you
need to process fields one-by-one before all fields have been consumed.

```rust,no_run
use axum_extra::extract::multipart::Field;
use modo::extractor::UploadedFile;

async fn process_field(field: Field) -> modo::Result<()> {
    let file = UploadedFile::from_field(field).await?;
    println!("received {} ({} bytes)", file.name, file.size);
    Ok(())
}
```
