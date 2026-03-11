use proc_macro::TokenStream;

mod from_multipart;

/// Derive macro for parsing `multipart/form-data` into a struct.
///
/// Supports `UploadedFile`, `Option<UploadedFile>`, `Vec<UploadedFile>`,
/// `BufferedUpload`, `String`, `Option<String>`, and other `FromStr` types.
///
/// Use `#[upload(max_size = "5mb", accept = "image/*")]` on file fields.
/// Use `#[serde(rename = "...")]` to rename the multipart field.
#[proc_macro_derive(FromMultipart, attributes(upload, serde))]
pub fn derive_from_multipart(input: TokenStream) -> TokenStream {
    from_multipart::expand(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
