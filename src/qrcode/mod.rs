//! QR code generation with customizable SVG output.
//!
//! Generates QR codes from any string and renders them as SVG with
//! configurable module shapes, finder shapes, and colors. Primary use
//! case is TOTP authenticator enrollment, but the module is
//! general-purpose.
//!
//! # Key types
//!
//! - [`QrCode`] — generated QR matrix; call [`QrCode::to_svg`] to render.
//! - [`QrStyle`] — rendering options: module shape, finder shape, colors, sizes.
//! - [`Ecl`] — error correction level (Low / Medium / Quartile / High).
//! - [`Color`] — foreground/background color as hex string or RGB tuple.
//! - [`ModuleShape`] — shape of data modules (Square, RoundedSquare, Circle, Diamond).
//! - [`FinderShape`] — shape of finder patterns (Square, Rounded, Circle).
//! - [`QrError`] — error type for generation and rendering failures.
//! - [`qr_svg_function`] — MiniJinja template function factory (requires
//!   `templates` feature).
//!
//! # Example
//!
//! ```
//! use modo::qrcode::{QrCode, QrStyle};
//!
//! let qr = QrCode::new("https://example.com").unwrap();
//! let svg = qr.to_svg(&QrStyle::default()).unwrap();
//! assert!(svg.starts_with("<svg"));
//! ```

mod code;
mod error;
mod render;
mod style;

pub use code::{Ecl, QrCode};
pub use error::QrError;
pub use style::{Color, FinderShape, ModuleShape, QrStyle};

mod template;

pub use template::qr_svg_function;
