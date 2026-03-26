//! QR code generation with customizable SVG output.
//!
//! Requires feature `"qrcode"`.
//!
//! Generates QR codes from any string and renders them as SVG with
//! configurable module shapes, finder shapes, and colors. Primary use
//! case is TOTP authenticator enrollment, but the module is
//! general-purpose.
//!
//! # Example
//!
//! ```rust,ignore
//! use modo::qrcode::{QrCode, QrStyle};
//!
//! let qr = QrCode::new("https://example.com").unwrap();
//! let svg = qr.to_svg(&QrStyle::default());
//! ```

mod code;
mod error;
mod render;
mod style;

pub use code::{Ecl, QrCode};
pub use error::QrError;
pub use style::{Color, FinderShape, ModuleShape, QrStyle};

#[cfg(feature = "templates")]
mod template;

#[cfg(feature = "templates")]
pub use template::qr_svg_function;
