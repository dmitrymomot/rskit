use proc_macro::TokenStream;

mod from_multipart;

/// Derive macro for parsing `multipart/form-data` into a struct.
///
/// Can only be derived for structs with named fields. Generates an implementation
/// of `modo_upload::FromMultipart`, which is used by the `MultipartForm` extractor.
///
/// # Supported field types
///
/// | Rust type               | Behaviour                                      |
/// |-------------------------|------------------------------------------------|
/// | `UploadedFile`          | Required file field; errors if absent          |
/// | `Option<UploadedFile>`  | Optional file field                            |
/// | `Vec<UploadedFile>`     | Zero or more files under the same field name   |
/// | `BufferedUpload`        | Required buffered upload (only one per struct) |
/// | `String`                | Required text field                            |
/// | `Option<String>`        | Optional text field                            |
/// | any `T: FromStr`        | Required text field, parsed via `FromStr`      |
///
/// # Field attributes
///
/// ## `#[upload(...)]`
///
/// Controls per-field validation. All sub-attributes are optional.
///
/// - `max_size = "<size>"` — maximum file size for `UploadedFile`, `Option<UploadedFile>`,
///   or `Vec<UploadedFile>` fields, e.g. `"5mb"`, `"100kb"`, `"2gb"`.
///   Size strings are case-insensitive and accept the suffixes `b`, `kb`, `mb`, `gb`.
///   A plain integer is treated as bytes.
/// - `accept = "<pattern>"` — MIME type pattern for `UploadedFile`, `Option<UploadedFile>`,
///   or `Vec<UploadedFile>` fields, e.g. `"image/*"`, `"application/pdf"`.
/// - `min_count = <n>` — minimum number of files for `Vec<UploadedFile>` fields.
/// - `max_count = <n>` — maximum number of files for `Vec<UploadedFile>` fields.
///
/// `max_size` and `accept` are not applied to `BufferedUpload` fields; use the
/// global `max_file_size` from `UploadConfig` to limit buffered upload size.
///
/// ## `#[serde(rename = "...")]`
///
/// Overrides the multipart field name used for matching. By default the Rust field name
/// is used as the multipart field name.
///
/// # Example
///
/// ```rust,ignore
/// use modo_upload::{FromMultipart, UploadedFile};
///
/// #[derive(FromMultipart)]
/// struct ProfileForm {
///     #[upload(max_size = "5mb", accept = "image/*")]
///     avatar: UploadedFile,
///
///     name: String,
///
///     #[upload(min_count = 1, max_count = 5)]
///     attachments: Vec<UploadedFile>,
///
///     #[serde(rename = "user_email")]
///     email: Option<String>,
/// }
/// ```
#[proc_macro_derive(FromMultipart, attributes(upload, serde))]
pub fn derive_from_multipart(input: TokenStream) -> TokenStream {
    from_multipart::expand(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
