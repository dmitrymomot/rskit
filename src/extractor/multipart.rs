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

impl UploadedFile {
    /// Build an `UploadedFile` from an axum multipart field.
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

    /// File extension from the original filename (lowercase, without dot).
    /// Returns `None` if no extension present.
    pub fn extension(&self) -> Option<String> {
        let ext = self.name.rsplit('.').next()?;
        if ext == self.name {
            None
        } else {
            Some(ext.to_ascii_lowercase())
        }
    }

    /// Start building a fluent validation chain for this file.
    pub fn validate(&self) -> crate::extractor::upload_validator::UploadValidator<'_> {
        crate::extractor::upload_validator::UploadValidator::new(self)
    }
}

/// A map of field-name to uploaded files.
///
/// Use `get()` for a shared reference, `file()` to take one file,
/// or `files()` to take all files for a given field name.
pub struct Files(HashMap<String, Vec<UploadedFile>>);

impl Files {
    /// Create a `Files` from a pre-built map.
    pub fn from_map(map: HashMap<String, Vec<UploadedFile>>) -> Self {
        Self(map)
    }

    /// Get a shared reference to the first file under `name`, if any.
    pub fn get(&self, name: &str) -> Option<&UploadedFile> {
        self.0.get(name).and_then(|v| v.first())
    }

    /// Take and return the first file under `name`, removing the entry
    /// if no files remain.
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

    /// Take and return all files under `name`.
    pub fn files(&mut self, name: &str) -> Vec<UploadedFile> {
        self.0.remove(name).unwrap_or_default()
    }
}

/// Extractor that parses a `multipart/form-data` request into a
/// deserialized+sanitized value `T` (from text fields) and a `Files`
/// map (from file fields).
///
/// ```ignore
/// async fn upload(
///     MultipartRequest(profile, files): MultipartRequest<Profile>,
/// ) -> impl IntoResponse { ... }
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
        let mut value: T = serde_urlencoded::from_str(&encoded).map_err(|e| {
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
