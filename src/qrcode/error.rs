use std::fmt;

/// Errors that can occur during QR code generation or rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QrError {
    /// Input data exceeds QR code capacity for the chosen error correction level.
    DataTooLong,
    /// Invalid hex color string.
    InvalidColor(String),
}

impl QrError {
    /// Returns a stable, namespaced string code for this error.
    pub fn code(&self) -> &'static str {
        match self {
            Self::DataTooLong => "qrcode:data_too_long",
            Self::InvalidColor(_) => "qrcode:invalid_color",
        }
    }
}

impl fmt::Display for QrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataTooLong => write!(f, "input data exceeds QR code capacity"),
            Self::InvalidColor(c) => write!(f, "invalid hex color: {c}"),
        }
    }
}

impl std::error::Error for QrError {}

impl From<QrError> for crate::Error {
    fn from(err: QrError) -> Self {
        let code = err.code();
        crate::Error::bad_request(err.to_string())
            .chain(err)
            .with_code(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_variants_have_unique_codes() {
        let variants = [
            QrError::DataTooLong,
            QrError::InvalidColor("#bad".into()),
        ];
        let mut codes: Vec<&str> = variants.iter().map(|v| v.code()).collect();
        let len_before = codes.len();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), len_before, "duplicate error codes found");
    }

    #[test]
    fn all_codes_start_with_qrcode_prefix() {
        let variants = [
            QrError::DataTooLong,
            QrError::InvalidColor("x".into()),
        ];
        for v in &variants {
            assert!(
                v.code().starts_with("qrcode:"),
                "code {} missing prefix",
                v.code()
            );
        }
    }

    #[test]
    fn display_is_human_readable() {
        assert_eq!(
            QrError::DataTooLong.to_string(),
            "input data exceeds QR code capacity"
        );
        assert_eq!(
            QrError::InvalidColor("#xyz".into()).to_string(),
            "invalid hex color: #xyz"
        );
    }

    #[test]
    fn converts_to_modo_error() {
        let err: crate::Error = QrError::DataTooLong.into();
        assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
        assert_eq!(err.error_code(), Some("qrcode:data_too_long"));
    }

    #[test]
    fn recoverable_via_source_as() {
        let err: crate::Error = QrError::InvalidColor("#bad".into()).into();
        let qr_err = err.source_as::<QrError>();
        assert_eq!(qr_err, Some(&QrError::InvalidColor("#bad".into())));
    }
}
