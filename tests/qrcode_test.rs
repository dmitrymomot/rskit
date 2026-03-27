#![cfg(feature = "qrcode")]

use modo::qrcode::{Color, Ecl, FinderShape, ModuleShape, QrCode, QrError, QrStyle};

#[test]
fn generates_svg_from_url() {
    let qr = QrCode::new("https://example.com").unwrap();
    let svg = qr.to_svg(&QrStyle::default()).unwrap();
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("viewBox"));
    assert!(svg.ends_with("</svg>"));
}

#[test]
fn generates_svg_from_short_text() {
    let qr = QrCode::new("hello").unwrap();
    let svg = qr.to_svg(&QrStyle::default()).unwrap();
    assert!(svg.contains("<svg"));
}

#[test]
fn generates_svg_from_empty_string() {
    let qr = QrCode::new("").unwrap();
    let svg = qr.to_svg(&QrStyle::default()).unwrap();
    assert!(svg.contains("<svg"));
}

#[test]
fn rejects_oversized_data() {
    let data = "x".repeat(10_000);
    let err = QrCode::new(&data).unwrap_err();
    assert_eq!(err, QrError::DataTooLong);
}

#[test]
fn ecl_affects_capacity() {
    let data = "x".repeat(500);
    // Low ECL has more capacity than High
    let low = QrCode::with_ecl(&data, Ecl::Low);
    let high = QrCode::with_ecl(&data, Ecl::High);

    // Both should succeed for moderate data
    assert!(low.is_ok());
    // High ECL may produce a larger matrix
    if let (Ok(l), Ok(h)) = (low, high) {
        assert!(h.size() >= l.size());
    }
}

#[test]
fn size_returns_positive_dimension() {
    let qr = QrCode::new("test").unwrap();
    assert!(qr.size() > 0);
    // QR Version 1 is 21x21 minimum
    assert!(qr.size() >= 21);
}

#[test]
fn custom_style_renders_svg() {
    let qr = QrCode::new("styled").unwrap();
    let style = QrStyle {
        module_shape: ModuleShape::Circle,
        finder_shape: FinderShape::Circle,
        fg_color: Color::Rgb(26, 26, 46),
        bg_color: Color::Hex("#ffffff".to_string()),
        module_size: 8,
        quiet_zone: 2,
    };
    let svg = qr.to_svg(&style).unwrap();
    assert!(svg.contains("<svg"));
    // Circle modules produce <circle> elements
    assert!(svg.contains("<circle"));
}

#[test]
fn diamond_module_shape_renders() {
    let qr = QrCode::new("diamond").unwrap();
    let style = QrStyle {
        module_shape: ModuleShape::Diamond,
        ..Default::default()
    };
    let svg = qr.to_svg(&style).unwrap();
    assert!(svg.contains("<svg"));
}

#[test]
fn square_shapes_render() {
    let qr = QrCode::new("square").unwrap();
    let style = QrStyle {
        module_shape: ModuleShape::Square,
        finder_shape: FinderShape::Square,
        ..Default::default()
    };
    let svg = qr.to_svg(&style).unwrap();
    assert!(svg.contains("<svg"));
}

#[test]
fn rounded_square_with_custom_radius() {
    let qr = QrCode::new("rounded").unwrap();
    let style = QrStyle {
        module_shape: ModuleShape::RoundedSquare { radius: 0.5 },
        ..Default::default()
    };
    let svg = qr.to_svg(&style).unwrap();
    assert!(svg.contains("<svg"));
}

#[test]
fn invalid_hex_color_produces_error() {
    let qr = QrCode::new("test").unwrap();
    let style = QrStyle {
        fg_color: Color::Hex("not-a-color".to_string()),
        ..Default::default()
    };
    let err = qr.to_svg(&style).unwrap_err();
    assert!(matches!(err, QrError::InvalidColor(_)));
}

#[test]
fn color_hex_shorthand_expands() {
    let color = Color::Hex("#fff".to_string());
    assert_eq!(color.to_hex().unwrap(), "#ffffff");
}

#[test]
fn color_rgb_converts_to_hex() {
    let color = Color::Rgb(255, 0, 128);
    assert_eq!(color.to_hex().unwrap(), "#ff0080");
}

#[test]
fn qr_error_converts_to_modo_error() {
    let err: modo::Error = QrError::DataTooLong.into();
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    assert_eq!(err.error_code(), Some("qrcode:data_too_long"));
}

#[test]
fn qr_error_invalid_color_converts() {
    let err: modo::Error = QrError::InvalidColor("bad".to_string()).into();
    assert_eq!(err.status(), http::StatusCode::BAD_REQUEST);
    assert_eq!(err.error_code(), Some("qrcode:invalid_color"));
}
