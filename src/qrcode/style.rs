use std::fmt;

use crate::qrcode::error::QrError;

/// Shape of individual data modules (the small squares/dots).
#[derive(Debug, Clone, PartialEq)]
pub enum ModuleShape {
    /// Classic sharp-edged square.
    Square,
    /// Square with rounded corners. `radius` is a fraction of module size (0.0–0.5), clamped at render time.
    RoundedSquare { radius: f32 },
    /// Circular dot.
    Circle,
    /// 45-degree rotated square (diamond).
    Diamond,
}

/// Shape of the three finder patterns (7×7 corner markers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinderShape {
    /// Classic concentric squares.
    Square,
    /// Concentric rounded rectangles.
    Rounded,
    /// Concentric circles.
    Circle,
}

/// A color value for QR code rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Color {
    /// Hex string: `"#000000"` (6-char) or `"#000"` (3-char shorthand).
    Hex(String),
    /// RGB components.
    Rgb(u8, u8, u8),
}

impl Color {
    /// Resolves the color to a 6-character hex string (e.g. `"#1a1a2e"`).
    ///
    /// Returns `QrError::InvalidColor` for malformed hex values.
    pub fn to_hex(&self) -> Result<String, QrError> {
        match self {
            Color::Rgb(r, g, b) => Ok(format!("#{r:02x}{g:02x}{b:02x}")),
            Color::Hex(s) => {
                let s = s.trim();
                if !s.starts_with('#') {
                    return Err(QrError::InvalidColor(s.to_string()));
                }
                let hex = &s[1..];
                match hex.len() {
                    3 => {
                        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                            return Err(QrError::InvalidColor(s.to_string()));
                        }
                        let expanded: String = hex.chars().flat_map(|c| [c, c]).collect();
                        Ok(format!("#{expanded}"))
                    }
                    6 => {
                        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                            return Err(QrError::InvalidColor(s.to_string()));
                        }
                        Ok(s.to_lowercase())
                    }
                    _ => Err(QrError::InvalidColor(s.to_string())),
                }
            }
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_hex() {
            Ok(hex) => write!(f, "{hex}"),
            Err(_) => write!(f, "(invalid)"),
        }
    }
}

/// Styling options for QR code SVG rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct QrStyle {
    /// Shape of data modules.
    pub module_shape: ModuleShape,
    /// Shape of the three finder patterns.
    pub finder_shape: FinderShape,
    /// Foreground (dark module) color.
    pub fg_color: Color,
    /// Background color.
    pub bg_color: Color,
    /// Size of each module in SVG units (pixels). Default: 10.
    pub module_size: u32,
    /// Number of quiet-zone modules around the QR code. Default: 4 (spec minimum).
    pub quiet_zone: u32,
}

impl Default for QrStyle {
    fn default() -> Self {
        Self {
            module_shape: ModuleShape::RoundedSquare { radius: 0.3 },
            finder_shape: FinderShape::Rounded,
            fg_color: Color::Hex("#000000".into()),
            bg_color: Color::Hex("#ffffff".into()),
            module_size: 10,
            quiet_zone: 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Color ---

    #[test]
    fn rgb_to_hex() {
        let c = Color::Rgb(26, 26, 46);
        assert_eq!(c.to_hex().unwrap(), "#1a1a2e");
    }

    #[test]
    fn rgb_black() {
        let c = Color::Rgb(0, 0, 0);
        assert_eq!(c.to_hex().unwrap(), "#000000");
    }

    #[test]
    fn rgb_white() {
        let c = Color::Rgb(255, 255, 255);
        assert_eq!(c.to_hex().unwrap(), "#ffffff");
    }

    #[test]
    fn hex_six_char_valid() {
        let c = Color::Hex("#1a1a2e".into());
        assert_eq!(c.to_hex().unwrap(), "#1a1a2e");
    }

    #[test]
    fn hex_six_char_uppercase_normalized() {
        let c = Color::Hex("#FF00AA".into());
        assert_eq!(c.to_hex().unwrap(), "#ff00aa");
    }

    #[test]
    fn hex_three_char_expanded() {
        let c = Color::Hex("#fff".into());
        assert_eq!(c.to_hex().unwrap(), "#ffffff");
    }

    #[test]
    fn hex_three_char_color() {
        let c = Color::Hex("#f0a".into());
        assert_eq!(c.to_hex().unwrap(), "#ff00aa");
    }

    #[test]
    fn hex_missing_hash_is_error() {
        let c = Color::Hex("000000".into());
        assert!(c.to_hex().is_err());
    }

    #[test]
    fn hex_invalid_chars_is_error() {
        let c = Color::Hex("#gggggg".into());
        assert!(c.to_hex().is_err());
    }

    #[test]
    fn hex_wrong_length_is_error() {
        let c = Color::Hex("#12345".into());
        assert!(c.to_hex().is_err());
    }

    #[test]
    fn hex_named_color_is_error() {
        let c = Color::Hex("red".into());
        assert!(c.to_hex().is_err());
    }

    #[test]
    fn hex_three_char_invalid_chars_is_error() {
        let c = Color::Hex("#ggg".into());
        assert!(c.to_hex().is_err());
    }

    // --- QrStyle defaults ---

    #[test]
    fn default_style_has_rounded_module_shape() {
        let s = QrStyle::default();
        assert_eq!(s.module_shape, ModuleShape::RoundedSquare { radius: 0.3 });
    }

    #[test]
    fn default_style_has_rounded_finder_shape() {
        let s = QrStyle::default();
        assert_eq!(s.finder_shape, FinderShape::Rounded);
    }

    #[test]
    fn default_style_colors() {
        let s = QrStyle::default();
        assert_eq!(s.fg_color.to_hex().unwrap(), "#000000");
        assert_eq!(s.bg_color.to_hex().unwrap(), "#ffffff");
    }

    #[test]
    fn default_style_sizes() {
        let s = QrStyle::default();
        assert_eq!(s.module_size, 10);
        assert_eq!(s.quiet_zone, 4);
    }
}
