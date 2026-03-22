/// Options for `Storage::put_with()`.
#[derive(Debug, Clone, Default)]
pub struct PutOptions {
    /// Sets the `Content-Disposition` header (e.g. `"attachment"`).
    pub content_disposition: Option<String>,
    /// Sets the `Cache-Control` header (e.g. `"max-age=31536000"`).
    pub cache_control: Option<String>,
    /// Overrides the file's content type. If `None`, uses `UploadedFile.content_type`.
    pub content_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_all_none() {
        let opts = PutOptions::default();
        assert!(opts.content_disposition.is_none());
        assert!(opts.cache_control.is_none());
        assert!(opts.content_type.is_none());
    }
}
