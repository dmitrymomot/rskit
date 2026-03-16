/// Trait for parsing a struct from `multipart/form-data`.
///
/// Implement this trait (or derive it with `#[derive(FromMultipart)]`) to
/// describe how multipart fields map to struct fields.  The
/// [`MultipartForm`](crate::MultipartForm) extractor calls this automatically during request
/// extraction.
pub trait FromMultipart: Sized {
    /// Parse `multipart` into `Self`, enforcing `max_file_size` on every file
    /// field when `Some`.
    fn from_multipart(
        multipart: &mut axum::extract::Multipart,
        max_file_size: Option<usize>,
    ) -> impl std::future::Future<Output = Result<Self, modo::Error>> + Send;
}
