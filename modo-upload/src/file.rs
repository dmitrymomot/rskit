use crate::validate::UploadValidator;

/// Extract the file extension from a filename (without dot, not lowercased).
///
/// Returns `None` when the filename has no dot or when the dot is at position 0
/// and the entire string equals the "extension" (i.e. the empty string `""`
/// — the empty-filename case).  Dotfiles like `".gitignore"` return
/// `Some("gitignore")`.  Filenames with no dot (e.g. `"noext"`) return `None`.
pub(crate) fn extract_extension(filename: &str) -> Option<&str> {
    let ext = filename.rsplit('.').next()?;
    if ext == filename { None } else { Some(ext) }
}

/// Metadata extracted from a multipart field (shared by UploadedFile and BufferedUpload).
pub(crate) struct FieldMeta {
    pub name: String,
    pub file_name: String,
    pub content_type: String,
}

impl FieldMeta {
    pub fn from_field(field: &axum::extract::multipart::Field<'_>) -> Self {
        Self {
            name: field.name().unwrap_or_default().to_owned(),
            file_name: field.file_name().unwrap_or_default().to_owned(),
            content_type: field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_owned(),
        }
    }
}

/// An uploaded file fully buffered in memory.
///
/// `UploadedFile` holds all bytes in a single [`bytes::Bytes`] buffer after the
/// multipart field has been drained.  For large files that should be streamed
/// rather than fully buffered, use [`BufferedUpload`](crate::BufferedUpload) instead.
pub struct UploadedFile {
    name: String,
    file_name: String,
    content_type: String,
    data: bytes::Bytes,
}

impl UploadedFile {
    /// Create from an axum multipart field (consumes the field).
    #[doc(hidden)]
    pub async fn from_field(
        field: axum::extract::multipart::Field<'_>,
        max_size: Option<usize>,
    ) -> Result<Self, modo::Error> {
        let meta = FieldMeta::from_field(&field);
        let mut field = field;
        let mut buf = bytes::BytesMut::new();
        while let Some(chunk) = field.chunk().await.map_err(|e| {
            modo::HttpError::BadRequest.with_message(format!("failed to read multipart chunk: {e}"))
        })? {
            buf.extend_from_slice(&chunk);
            if let Some(max) = max_size
                && buf.len() > max
            {
                return Err(modo::HttpError::PayloadTooLarge
                    .with_message("upload exceeds maximum allowed size"));
            }
        }
        Ok(Self {
            name: meta.name,
            file_name: meta.file_name,
            content_type: meta.content_type,
            data: buf.freeze(),
        })
    }

    /// The multipart field name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The original filename provided by the client.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// The MIME content type.
    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    /// The raw file bytes.
    pub fn data(&self) -> &bytes::Bytes {
        &self.data
    }

    /// File size in bytes.
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// File extension from the original filename (lowercase, without dot).
    pub fn extension(&self) -> Option<String> {
        extract_extension(&self.file_name).map(|ext| ext.to_ascii_lowercase())
    }

    /// Whether the file is empty (zero bytes).
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Test helper — construct an `UploadedFile` without multipart parsing.
    #[doc(hidden)]
    pub fn __test_new(name: &str, file_name: &str, content_type: &str, data: &[u8]) -> Self {
        Self {
            name: name.to_owned(),
            file_name: file_name.to_owned(),
            content_type: content_type.to_owned(),
            data: bytes::Bytes::copy_from_slice(data),
        }
    }

    /// Start building a fluent validation chain for this file.
    ///
    /// Returns an `UploadValidator` that lets you chain `.max_size()` and
    /// `.accept()` calls before calling `.check()` to get the final result.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use modo_upload::mb;
    ///
    /// file.validate()
    ///     .max_size(mb(5))
    ///     .accept("image/*")
    ///     .check()?;
    /// ```
    pub fn validate(&self) -> UploadValidator<'_> {
        UploadValidator::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_with_name(file_name: &str) -> UploadedFile {
        UploadedFile::__test_new("f", file_name, "application/octet-stream", b"")
    }

    #[test]
    fn extension_lowercase() {
        assert_eq!(file_with_name("photo.JPG").extension(), Some("jpg".into()));
    }

    #[test]
    fn extension_compound() {
        assert_eq!(
            file_with_name("archive.tar.gz").extension(),
            Some("gz".into())
        );
    }

    #[test]
    fn extension_dotfile() {
        assert_eq!(
            file_with_name(".gitignore").extension(),
            Some("gitignore".into())
        );
    }

    #[test]
    fn extension_none() {
        assert_eq!(file_with_name("noext").extension(), None);
    }

    #[test]
    fn extension_trailing_dot() {
        assert_eq!(file_with_name("file.").extension(), Some("".into()));
    }

    #[test]
    fn extension_empty_filename() {
        assert_eq!(file_with_name("").extension(), None);
    }

    #[test]
    fn extension_only_dots() {
        assert_eq!(file_with_name("....").extension(), Some("".into()));
    }

    #[test]
    fn extension_single_dot() {
        assert_eq!(file_with_name(".").extension(), Some("".into()));
    }

    #[test]
    fn extension_unicode_filename() {
        assert_eq!(file_with_name("café.txt").extension(), Some("txt".into()));
    }

    #[test]
    fn extension_space_in_name() {
        assert_eq!(
            file_with_name("my file.tar.gz").extension(),
            Some("gz".into())
        );
    }

    #[test]
    fn accessors_nonempty_file() {
        let f = UploadedFile::__test_new("field", "photo.jpg", "image/jpeg", b"imgdata");
        assert_eq!(f.name(), "field");
        assert_eq!(f.file_name(), "photo.jpg");
        assert_eq!(f.content_type(), "image/jpeg");
        assert_eq!(f.data().as_ref(), b"imgdata");
        assert_eq!(f.size(), 7);
        assert!(!f.is_empty());
    }

    #[test]
    fn accessors_empty_file() {
        let f = UploadedFile::__test_new("field", "empty.bin", "application/octet-stream", b"");
        assert_eq!(f.size(), 0);
        assert!(f.is_empty());
    }
}
