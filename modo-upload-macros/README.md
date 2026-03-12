# modo-upload-macros

Procedural macro crate for `modo-upload`. Provides the `#[derive(FromMultipart)]` macro that generates
`modo_upload::FromMultipart` implementations for structs, enabling automatic parsing and validation of
`multipart/form-data` requests.

This crate is an implementation detail of `modo-upload`. Add `modo-upload` to your dependencies — it
re-exports `FromMultipart` and you interact with the macro through that crate.

## Usage

### Basic form with a required file

```rust
use modo_upload::{FromMultipart, UploadedFile};

#[derive(FromMultipart)]
struct ProfileForm {
    #[upload(max_size = "5mb", accept = "image/*")]
    avatar: UploadedFile,

    name: String,
    email: String,
}
```

### Optional file and text fields

```rust
use modo_upload::{FromMultipart, UploadedFile};

#[derive(FromMultipart)]
struct UpdateForm {
    avatar: Option<UploadedFile>,
    bio: Option<String>,
}
```

### Multiple files with count constraints

```rust
use modo_upload::{FromMultipart, UploadedFile};

#[derive(FromMultipart)]
struct GalleryForm {
    #[upload(min_count = 1, max_count = 10, max_size = "10mb", accept = "image/*")]
    images: Vec<UploadedFile>,

    title: String,
}
```

### Renaming multipart fields

Use `#[serde(rename = "...")]` to map a Rust field to a differently named multipart field.

```rust
use modo_upload::{FromMultipart, UploadedFile};

#[derive(FromMultipart)]
struct UploadForm {
    #[serde(rename = "profile_picture")]
    avatar: UploadedFile,
}
```

### Using `FromStr` fields

Any type that implements `std::str::FromStr` can be used as a text field.

```rust
use modo_upload::FromMultipart;

#[derive(FromMultipart)]
struct OrderForm {
    quantity: u32,
    price: f64,
    label: String,
}
```

### Integration with `MultipartForm` extractor

In an axum/modo handler, use `MultipartForm<T>` from `modo_upload` to extract and validate the form:

```rust
use modo_upload::{FileStorage, FromMultipart, MultipartForm, UploadedFile};
use modo::extractors::service::Service;
use modo::JsonResult;

#[derive(FromMultipart)]
struct ProfileForm {
    #[upload(max_size = "5mb", accept = "image/*")]
    avatar: UploadedFile,
    name: String,
}

#[modo::handler(POST, "/profile")]
async fn update_profile(
    storage: Service<Box<dyn FileStorage>>,
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

## Field attributes reference

### `#[upload(...)]`

Applied to file fields (`UploadedFile`, `Option<UploadedFile>`, `Vec<UploadedFile>`, `BufferedUpload`).
All sub-attributes are optional and can be combined.

| Attribute              | Applies to               | Description                                                                                                 |
| ---------------------- | ------------------------ | ----------------------------------------------------------------------------------------------------------- |
| `max_size = "<size>"`  | all file types           | Maximum size per file. Accepts `b`, `kb`, `mb`, `gb` suffixes (case-insensitive). Plain integers are bytes. |
| `accept = "<pattern>"` | all file types           | MIME type pattern, e.g. `"image/*"`, `"application/pdf"`                                                    |
| `min_count = <n>`      | `Vec<UploadedFile>` only | Minimum number of uploaded files                                                                            |
| `max_count = <n>`      | `Vec<UploadedFile>` only | Maximum number of uploaded files                                                                            |

### `#[serde(rename = "...")]`

Overrides the multipart field name. By default the Rust field name is used.

## Supported field types

| Rust type              | Required | Notes                                            |
| ---------------------- | -------- | ------------------------------------------------ |
| `UploadedFile`         | yes      | Single file; validation error if missing         |
| `Option<UploadedFile>` | no       | Optional single file                             |
| `Vec<UploadedFile>`    | no       | Zero or more files under the same multipart name |
| `BufferedUpload`       | yes      | Streaming upload; at most one field per struct   |
| `String`               | yes      | Required text field                              |
| `Option<String>`       | no       | Optional text field                              |
| `T: FromStr`           | yes      | Required text field parsed via `FromStr`         |

## Constraints

- Only structs with named fields are supported.
- At most one `BufferedUpload` field is allowed per struct.
