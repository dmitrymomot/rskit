use crate::qrcode::error::QrError;
use crate::qrcode::render;
use crate::qrcode::style::QrStyle;

/// Error correction level for QR code generation.
///
/// Higher levels increase data recovery at the cost of larger QR codes.
/// [`QrCode::new`] defaults to [`Ecl::Medium`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecl {
    /// Low — recovers ~7% of data.
    Low,
    /// Medium — recovers ~15% of data. This is the default.
    Medium,
    /// Quartile — recovers ~25% of data.
    Quartile,
    /// High — recovers ~30% of data.
    High,
}

impl Ecl {
    fn to_fast_qr(self) -> fast_qr::ECL {
        match self {
            Self::Low => fast_qr::ECL::L,
            Self::Medium => fast_qr::ECL::M,
            Self::Quartile => fast_qr::ECL::Q,
            Self::High => fast_qr::ECL::H,
        }
    }
}

/// A generated QR code ready for SVG rendering.
///
/// Create with [`QrCode::new`] (default error correction) or
/// [`QrCode::with_ecl`] (explicit level), then render via
/// [`QrCode::to_svg`].
///
/// # Example
///
/// ```
/// use modo::qrcode::{QrCode, QrStyle, Ecl};
///
/// let qr = QrCode::with_ecl("https://example.com", Ecl::High).unwrap();
/// let svg = qr.to_svg(&QrStyle::default()).unwrap();
/// assert!(svg.starts_with("<svg"));
/// ```
#[derive(Debug)]
pub struct QrCode {
    pub(super) qr: fast_qr::QRCode,
}

impl QrCode {
    /// Generate a QR code matrix with default error correction ([`Ecl::Medium`]).
    ///
    /// Returns [`QrError::DataTooLong`] if the input exceeds QR capacity.
    pub fn new(data: &str) -> Result<Self, QrError> {
        Self::with_ecl(data, Ecl::Medium)
    }

    /// Generate a QR code matrix with the specified error correction level.
    ///
    /// Returns [`QrError::DataTooLong`] if the input exceeds QR capacity
    /// for the chosen level.
    pub fn with_ecl(data: &str, ecl: Ecl) -> Result<Self, QrError> {
        let qr = fast_qr::QRBuilder::new(data)
            .ecl(ecl.to_fast_qr())
            .build()
            .map_err(|_| QrError::DataTooLong)?;
        Ok(Self { qr })
    }

    /// Render the QR code as an SVG string.
    ///
    /// The SVG uses a `viewBox` (no fixed `width`/`height`) so it scales
    /// to its container. Returns [`QrError::InvalidColor`] if any color
    /// in `style` is malformed.
    pub fn to_svg(&self, style: &QrStyle) -> Result<String, QrError> {
        render::render_svg(&self.qr, style)
    }

    /// Returns the number of modules along one side of the QR matrix.
    ///
    /// This is the raw matrix dimension (e.g. 21 for Version 1) and does
    /// not include the quiet zone added during SVG rendering.
    pub fn size(&self) -> usize {
        self.qr.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_qrcode_from_url() {
        let qr = QrCode::new("https://example.com").unwrap();
        assert!(qr.size() > 0);
    }

    #[test]
    fn new_creates_qrcode_from_otpauth_uri() {
        let uri = "otpauth://totp/Example:user@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example";
        let qr = QrCode::new(uri).unwrap();
        assert!(qr.size() >= 21);
    }

    #[test]
    fn new_creates_qrcode_from_empty_string() {
        let qr = QrCode::new("");
        assert!(qr.is_ok() || matches!(qr, Err(QrError::DataTooLong)));
    }

    #[test]
    fn with_ecl_low() {
        let qr = QrCode::with_ecl("test", Ecl::Low).unwrap();
        assert!(qr.size() > 0);
    }

    #[test]
    fn with_ecl_high() {
        let qr = QrCode::with_ecl("test", Ecl::High).unwrap();
        assert!(qr.size() > 0);
    }

    #[test]
    fn higher_ecl_may_produce_larger_qr() {
        let low = QrCode::with_ecl(
            "Hello, World! This is some test data for QR codes.",
            Ecl::Low,
        )
        .unwrap();
        let high = QrCode::with_ecl(
            "Hello, World! This is some test data for QR codes.",
            Ecl::High,
        )
        .unwrap();
        assert!(high.size() >= low.size());
    }

    #[test]
    fn oversized_data_returns_data_too_long() {
        let data = "x".repeat(8000);
        let err = QrCode::new(&data).unwrap_err();
        assert_eq!(err, QrError::DataTooLong);
    }

    #[test]
    fn to_svg_produces_svg_string() {
        let qr = QrCode::new("test").unwrap();
        let svg = qr.to_svg(&QrStyle::default()).unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("viewBox"));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn size_returns_correct_dimension() {
        let qr = QrCode::new("A").unwrap();
        assert_eq!(qr.size(), 21);
    }
}
