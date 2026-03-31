/// Access control for uploaded objects.
///
/// Maps to the S3 `x-amz-acl` header. `None` in [`PutOptions`] means
/// the bucket default applies.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Acl {
    #[default]
    Private,
    PublicRead,
}

impl Acl {
    /// S3 `x-amz-acl` header value.
    pub fn as_header_value(&self) -> &'static str {
        match self {
            Acl::Private => "private",
            Acl::PublicRead => "public-read",
        }
    }
}

/// Options for [`Storage::put_with()`](super::Storage::put_with) and
/// [`Storage::put_from_url_with()`](super::Storage::put_from_url_with).
#[non_exhaustive]
#[derive(Debug, Clone, Default)]
pub struct PutOptions {
    /// Sets the `Content-Disposition` header (e.g. `"attachment"`).
    pub content_disposition: Option<String>,
    /// Sets the `Cache-Control` header (e.g. `"max-age=31536000"`).
    pub cache_control: Option<String>,
    /// Overrides the content type from [`PutInput`](super::PutInput). If `None`, the `PutInput.content_type` is used.
    pub content_type: Option<String>,
    /// Sets the S3 `x-amz-acl` header. If `None`, the bucket default applies.
    pub acl: Option<Acl>,
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

    #[test]
    fn acl_default_is_private() {
        assert_eq!(Acl::default(), Acl::Private);
    }

    #[test]
    fn acl_private_header_value() {
        assert_eq!(Acl::Private.as_header_value(), "private");
    }

    #[test]
    fn acl_public_read_header_value() {
        assert_eq!(Acl::PublicRead.as_header_value(), "public-read");
    }

    #[test]
    fn default_options_acl_is_none() {
        let opts = PutOptions::default();
        assert!(opts.acl.is_none());
    }
}
