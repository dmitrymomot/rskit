use std::collections::HashMap;

use axum::extract::FromRequest;
use http::Request;
use serde::de::DeserializeOwned;

use crate::error::Error;
use crate::sanitize::Sanitize;

/// A single uploaded file extracted from a multipart request.
pub struct UploadedFile {
    /// Original file name from the upload.
    pub name: String,
    /// MIME content type (defaults to `application/octet-stream`).
    pub content_type: String,
    /// Size in bytes.
    pub size: usize,
    /// Raw file bytes.
    pub data: bytes::Bytes,
}

impl std::fmt::Debug for UploadedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UploadedFile")
            .field("name", &self.name)
            .field("content_type", &self.content_type)
            .field("size", &self.size)
            .finish()
    }
}

impl UploadedFile {
    /// Build an `UploadedFile` by consuming an axum multipart field.
    ///
    /// Reads the entire field body into memory. Prefer using [`MultipartRequest`]
    /// rather than calling this directly; it is public for advanced use cases
    /// that need to process fields individually.
    ///
    /// # Errors
    ///
    /// Returns a `400 Bad Request` error if the field body cannot be read.
    pub async fn from_field(
        field: axum_extra::extract::multipart::Field,
    ) -> crate::error::Result<Self> {
        let name = field.file_name().unwrap_or("unnamed").to_string();
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let data = field
            .bytes()
            .await
            .map_err(|e| Error::bad_request(format!("failed to read file field: {e}")))?;
        let size = data.len();
        Ok(Self {
            name,
            content_type,
            size,
            data,
        })
    }

    /// Returns the file extension from the original filename in lowercase, without the leading dot.
    ///
    /// Returns `None` if the filename has no extension (e.g. `"readme"`) or is empty.
    /// For compound extensions such as `"archive.tar.gz"`, only the last component (`"gz"`)
    /// is returned.
    pub fn extension(&self) -> Option<String> {
        let ext = self.name.rsplit('.').next()?;
        if ext == self.name {
            None
        } else {
            Some(ext.to_ascii_lowercase())
        }
    }

    /// Start building a fluent validation chain for this file.
    ///
    /// Returns an [`UploadValidator`](crate::extractor::UploadValidator) that can be used to
    /// check size and content type. Call
    /// [`UploadValidator::check`](crate::extractor::UploadValidator::check) to finalize and
    /// collect any violations.
    pub fn validate(&self) -> crate::extractor::UploadValidator<'_> {
        crate::extractor::upload_validator::UploadValidator::new(self)
    }
}

/// A map of field names to their uploaded files, produced by [`MultipartRequest`].
///
/// Files are stored by the multipart field name. Multiple files with the same field
/// name are supported. Use [`Files::get`] for a shared reference to the first file,
/// [`Files::file`] to take ownership of the first file, or [`Files::files`] to take
/// all files for a given field name.
pub struct Files(HashMap<String, Vec<UploadedFile>>);

impl std::fmt::Debug for Files {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Files")
            .field("fields", &self.0.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Files {
    /// Create a [`Files`] collection from a pre-built map.
    pub fn from_map(map: HashMap<String, Vec<UploadedFile>>) -> Self {
        Self(map)
    }

    /// Get a shared reference to the first file under `name`, if any.
    pub fn get(&self, name: &str) -> Option<&UploadedFile> {
        self.0.get(name).and_then(|v| v.first())
    }

    /// Take ownership of the first file under `name`.
    ///
    /// Removes the field entry entirely if no files remain after the take.
    pub fn file(&mut self, name: &str) -> Option<UploadedFile> {
        let files = self.0.get_mut(name)?;
        if files.is_empty() {
            None
        } else {
            let file = files.remove(0);
            if files.is_empty() {
                self.0.remove(name);
            }
            Some(file)
        }
    }

    /// Take ownership of all files under `name`.
    ///
    /// Returns an empty `Vec` if `name` was not present.
    pub fn files(&mut self, name: &str) -> Vec<UploadedFile> {
        self.0.remove(name).unwrap_or_default()
    }
}

/// Axum extractor for `multipart/form-data` requests.
///
/// Splits the multipart body into text fields (deserialized and sanitized into `T`) and
/// file fields (collected into a [`Files`] map). The inner tuple is `(T, Files)`.
///
/// Text fields are re-encoded as `application/x-www-form-urlencoded` and deserialized
/// via `serde_qs` (form-encoding mode) before [`Sanitize::sanitize`] is called on the
/// result. Repeated text fields, nested structs (`address[city]=…`), and indexed-bracket
/// `Vec<Struct>` rows (`contacts[0][kind]=…`) all deserialize naturally — same behavior
/// as [`crate::extractor::FormRequest`]. File fields are fully buffered into memory as
/// [`UploadedFile`] values.
///
/// # Errors
///
/// The [`FromRequest::Rejection`] is [`crate::Error`]. A `400 Bad Request` is returned
/// if the request is not a valid `multipart/form-data` body, a field cannot be read,
/// or the collected text fields cannot be deserialized into `T`. The error renders via its
/// [`IntoResponse`](axum::response::IntoResponse) impl.
///
/// # Example
///
/// ```rust,no_run
/// use modo::extractor::{MultipartRequest, Files};
/// use modo::sanitize::Sanitize;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct ProfileForm {
///     display_name: String,
/// }
///
/// impl Sanitize for ProfileForm {
///     fn sanitize(&mut self) {
///         self.display_name = self.display_name.trim().to_string();
///     }
/// }
///
/// async fn update_profile(
///     MultipartRequest(form, mut files): MultipartRequest<ProfileForm>,
/// ) {
///     let avatar = files.file("avatar"); // Option<UploadedFile>
/// }
/// ```
pub struct MultipartRequest<T>(pub T, pub Files);

impl<S, T> FromRequest<S> for MultipartRequest<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Sanitize,
{
    type Rejection = Error;

    async fn from_request(
        req: Request<axum::body::Body>,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let mut multipart = axum_extra::extract::Multipart::from_request(req, state)
            .await
            .map_err(|e| Error::bad_request(format!("invalid multipart request: {e}")))?;

        let mut text_fields: Vec<(String, String)> = Vec::new();
        let mut file_fields: HashMap<String, Vec<UploadedFile>> = HashMap::new();

        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|e| Error::bad_request(format!("failed to read multipart field: {e}")))?
        {
            let field_name = field.name().unwrap_or("").to_string();

            if field.file_name().is_some() {
                let uploaded = UploadedFile::from_field(field).await?;
                file_fields.entry(field_name).or_default().push(uploaded);
            } else {
                let text = field
                    .text()
                    .await
                    .map_err(|e| Error::bad_request(format!("failed to read text field: {e}")))?;
                text_fields.push((field_name, text));
            }
        }

        let encoded = serde_urlencoded::to_string(&text_fields).map_err(|e| {
            Error::bad_request(format!("failed to encode multipart text fields: {e}"))
        })?;
        let mut value: T = serde_qs::Config::new()
            .use_form_encoding(true)
            .deserialize_str(&encoded)
            .map_err(|e| {
                Error::bad_request(format!("failed to deserialize multipart text fields: {e}"))
            })?;
        value.sanitize();

        Ok(MultipartRequest(value, Files(file_fields)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_with_name(name: &str) -> UploadedFile {
        UploadedFile {
            name: name.to_string(),
            content_type: "application/octet-stream".to_string(),
            size: 0,
            data: bytes::Bytes::new(),
        }
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
    fn extension_none() {
        assert_eq!(file_with_name("noext").extension(), None);
    }

    #[test]
    fn extension_dotfile() {
        assert_eq!(
            file_with_name(".gitignore").extension(),
            Some("gitignore".into())
        );
    }

    #[test]
    fn extension_empty_filename() {
        assert_eq!(file_with_name("").extension(), None);
    }
}
