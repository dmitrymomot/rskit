use super::facade::PutInput;
use crate::extractor::UploadedFile;

impl PutInput {
    /// Build from an [`UploadedFile`] and a storage prefix.
    pub fn from_upload(file: &UploadedFile, prefix: &str) -> Self {
        let filename = if file.name.is_empty() {
            None
        } else {
            Some(file.name.clone())
        };
        Self {
            data: file.data.clone(),
            prefix: prefix.to_string(),
            filename,
            content_type: file.content_type.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn test_file(name: &str, ct: &str) -> UploadedFile {
        UploadedFile {
            name: name.to_string(),
            content_type: ct.to_string(),
            size: 5,
            data: Bytes::from_static(b"hello"),
        }
    }

    #[test]
    fn from_upload_copies_fields() {
        let file = test_file("photo.jpg", "image/jpeg");
        let input = PutInput::from_upload(&file, "avatars/");
        assert_eq!(input.prefix, "avatars/");
        assert_eq!(input.filename, Some("photo.jpg".to_string()));
        assert_eq!(input.content_type, "image/jpeg");
        assert_eq!(input.data.len(), 5);
    }

    #[test]
    fn from_upload_empty_name_becomes_none() {
        let file = test_file("", "application/octet-stream");
        let input = PutInput::from_upload(&file, "uploads/");
        assert_eq!(input.filename, None);
    }

    #[test]
    fn from_upload_unnamed_preserved() {
        let file = test_file("unnamed", "application/octet-stream");
        let input = PutInput::from_upload(&file, "uploads/");
        assert_eq!(input.filename, Some("unnamed".to_string()));
    }
}
