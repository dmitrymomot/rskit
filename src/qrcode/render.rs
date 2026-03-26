use std::fmt::Write;

use crate::qrcode::error::QrError;
use crate::qrcode::style::{FinderShape, ModuleShape, QrStyle};

/// Render a QR code matrix as an SVG string.
pub(crate) fn render_svg(qr: &fast_qr::QRCode, style: &QrStyle) -> Result<String, QrError> {
    let fg = style.fg_color.to_hex()?;
    let bg = style.bg_color.to_hex()?;
    let m = style.module_size as f64;
    let q = style.quiet_zone as f64 * m;
    let total = qr.size as f64 * m + 2.0 * q;

    let mut svg = String::with_capacity(4096);

    // SVG header — viewBox only, no width/height
    write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {total} {total}">"#,
    )
    .unwrap();

    // Background
    write!(svg, r#"<rect fill="{bg}" width="100%" height="100%"/>"#,).unwrap();

    // Render the three finder patterns as grouped elements
    render_finder(style, &mut svg, &fg, 0, 0)?; // top-left
    render_finder(style, &mut svg, &fg, 0, qr.size - 7)?; // top-right
    render_finder(style, &mut svg, &fg, qr.size - 7, 0)?; // bottom-left

    // Render data modules (skip finder regions)
    for row in 0..qr.size {
        for col in 0..qr.size {
            if is_finder_region(row, col, qr.size) {
                continue;
            }
            if !qr[row][col].value() {
                continue; // light module — skip
            }
            let x = q + col as f64 * m;
            let y = q + row as f64 * m;
            render_module(&mut svg, &style.module_shape, x, y, m, &fg);
        }
    }

    svg.push_str("</svg>");
    Ok(svg)
}

/// Check if a cell falls within one of the three 7×7 finder pattern regions.
fn is_finder_region(row: usize, col: usize, size: usize) -> bool {
    let in_top_left = row < 7 && col < 7;
    let in_top_right = row < 7 && col >= size - 7;
    let in_bottom_left = row >= size - 7 && col < 7;
    in_top_left || in_top_right || in_bottom_left
}

/// Render a single data module based on the chosen shape.
fn render_module(svg: &mut String, shape: &ModuleShape, x: f64, y: f64, m: f64, fg: &str) {
    match shape {
        ModuleShape::Square => {
            write!(
                svg,
                r#"<rect x="{x}" y="{y}" width="{m}" height="{m}" fill="{fg}"/>"#
            )
            .unwrap();
        }
        ModuleShape::RoundedSquare { radius } => {
            let r = radius.clamp(0.0, 0.5) as f64 * m;
            write!(
                svg,
                r#"<rect x="{x}" y="{y}" width="{m}" height="{m}" rx="{r}" ry="{r}" fill="{fg}"/>"#,
            )
            .unwrap();
        }
        ModuleShape::Circle => {
            let cx = x + m / 2.0;
            let cy = y + m / 2.0;
            let r = m / 2.0;
            write!(svg, r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{fg}"/>"#).unwrap();
        }
        ModuleShape::Diamond => {
            let cx = x + m / 2.0;
            write!(
                svg,
                r#"<polygon points="{cx},{y} {},{} {cx},{} {x},{}" fill="{fg}"/>"#,
                x + m,
                y + m / 2.0,
                y + m,
                y + m / 2.0,
            )
            .unwrap();
        }
    }
}

/// Render a 7×7 finder pattern as a group of concentric shapes.
fn render_finder(
    style: &QrStyle,
    svg: &mut String,
    fg: &str,
    start_row: usize,
    start_col: usize,
) -> Result<(), QrError> {
    let bg = style.bg_color.to_hex()?;
    let m = style.module_size as f64;
    let q = style.quiet_zone as f64 * m;
    let x = q + start_col as f64 * m;
    let y = q + start_row as f64 * m;

    svg.push_str("<g>");

    match style.finder_shape {
        FinderShape::Square => {
            // Outer 7×7
            let s7 = 7.0 * m;
            write!(
                svg,
                r#"<rect x="{x}" y="{y}" width="{s7}" height="{s7}" fill="{fg}"/>"#
            )
            .unwrap();
            // Middle 5×5 (background gap)
            let s5 = 5.0 * m;
            let x5 = x + m;
            let y5 = y + m;
            write!(
                svg,
                r#"<rect x="{x5}" y="{y5}" width="{s5}" height="{s5}" fill="{bg}"/>"#
            )
            .unwrap();
            // Inner 3×3
            let s3 = 3.0 * m;
            let x3 = x + 2.0 * m;
            let y3 = y + 2.0 * m;
            write!(
                svg,
                r#"<rect x="{x3}" y="{y3}" width="{s3}" height="{s3}" fill="{fg}"/>"#
            )
            .unwrap();
        }
        FinderShape::Rounded => {
            let r_outer = m * 0.5;
            let r_mid = m * 0.4;
            let r_inner = m * 0.3;

            let s7 = 7.0 * m;
            write!(
                svg,
                r#"<rect x="{x}" y="{y}" width="{s7}" height="{s7}" rx="{r_outer}" ry="{r_outer}" fill="{fg}"/>"#,
            )
            .unwrap();
            let s5 = 5.0 * m;
            let x5 = x + m;
            let y5 = y + m;
            write!(
                svg,
                r#"<rect x="{x5}" y="{y5}" width="{s5}" height="{s5}" rx="{r_mid}" ry="{r_mid}" fill="{bg}"/>"#,
            )
            .unwrap();
            let s3 = 3.0 * m;
            let x3 = x + 2.0 * m;
            let y3 = y + 2.0 * m;
            write!(
                svg,
                r#"<rect x="{x3}" y="{y3}" width="{s3}" height="{s3}" rx="{r_inner}" ry="{r_inner}" fill="{fg}"/>"#,
            )
            .unwrap();
        }
        FinderShape::Circle => {
            let center_x = x + 3.5 * m;
            let center_y = y + 3.5 * m;

            let r_outer = 3.5 * m;
            write!(
                svg,
                r#"<circle cx="{center_x}" cy="{center_y}" r="{r_outer}" fill="{fg}"/>"#,
            )
            .unwrap();
            let r_mid = 2.5 * m;
            write!(
                svg,
                r#"<circle cx="{center_x}" cy="{center_y}" r="{r_mid}" fill="{bg}"/>"#,
            )
            .unwrap();
            let r_inner = 1.5 * m;
            write!(
                svg,
                r#"<circle cx="{center_x}" cy="{center_y}" r="{r_inner}" fill="{fg}"/>"#,
            )
            .unwrap();
        }
    }

    svg.push_str("</g>");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qrcode::code::QrCode;
    use crate::qrcode::style::{Color, QrStyle};

    fn make_qr() -> QrCode {
        QrCode::new("https://example.com").unwrap()
    }

    #[test]
    fn svg_starts_with_tag() {
        let qr = make_qr();
        let svg = render_svg(&qr.qr, &QrStyle::default()).unwrap();
        assert!(svg.starts_with("<svg"));
    }

    #[test]
    fn svg_ends_with_closing_tag() {
        let qr = make_qr();
        let svg = render_svg(&qr.qr, &QrStyle::default()).unwrap();
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn svg_has_viewbox() {
        let qr = make_qr();
        let svg = render_svg(&qr.qr, &QrStyle::default()).unwrap();
        assert!(svg.contains("viewBox"));
    }

    #[test]
    fn svg_has_xmlns() {
        let qr = make_qr();
        let svg = render_svg(&qr.qr, &QrStyle::default()).unwrap();
        assert!(svg.contains(r#"xmlns="http://www.w3.org/2000/svg""#));
    }

    #[test]
    fn svg_has_background_rect() {
        let qr = make_qr();
        let svg = render_svg(&qr.qr, &QrStyle::default()).unwrap();
        assert!(svg.contains("fill=\"#ffffff\""));
        assert!(svg.contains("width=\"100%\""));
    }

    #[test]
    fn square_modules_produce_rect_elements() {
        let qr = make_qr();
        let style = QrStyle {
            module_shape: ModuleShape::Square,
            finder_shape: FinderShape::Square,
            ..Default::default()
        };
        let svg = render_svg(&qr.qr, &style).unwrap();
        assert!(svg.contains("<rect x="));
    }

    #[test]
    fn rounded_modules_produce_rx_ry() {
        let qr = make_qr();
        let style = QrStyle {
            module_shape: ModuleShape::RoundedSquare { radius: 0.3 },
            ..Default::default()
        };
        let svg = render_svg(&qr.qr, &style).unwrap();
        assert!(svg.contains("rx="));
        assert!(svg.contains("ry="));
    }

    #[test]
    fn circle_modules_produce_circle_elements() {
        let qr = make_qr();
        let style = QrStyle {
            module_shape: ModuleShape::Circle,
            ..Default::default()
        };
        let svg = render_svg(&qr.qr, &style).unwrap();
        let circle_count = svg.matches("<circle").count();
        assert!(
            circle_count > 3,
            "expected data circles, got {circle_count}"
        );
    }

    #[test]
    fn diamond_modules_produce_polygon_elements() {
        let qr = make_qr();
        let style = QrStyle {
            module_shape: ModuleShape::Diamond,
            ..Default::default()
        };
        let svg = render_svg(&qr.qr, &style).unwrap();
        assert!(svg.contains("<polygon"));
    }

    #[test]
    fn square_finders_produce_concentric_rects() {
        let qr = make_qr();
        let style = QrStyle {
            finder_shape: FinderShape::Square,
            module_shape: ModuleShape::Diamond,
            ..Default::default()
        };
        let svg = render_svg(&qr.qr, &style).unwrap();
        let g_count = svg.matches("<g>").count();
        assert_eq!(g_count, 3, "expected 3 finder groups");
    }

    #[test]
    fn rounded_finders_have_rounded_rects() {
        let qr = make_qr();
        let style = QrStyle {
            finder_shape: FinderShape::Rounded,
            module_shape: ModuleShape::Diamond,
            ..Default::default()
        };
        let svg = render_svg(&qr.qr, &style).unwrap();
        let g_start = svg.find("<g>").unwrap();
        let g_end = svg[g_start..].find("</g>").unwrap() + g_start;
        let finder_group = &svg[g_start..g_end];
        assert!(finder_group.contains("rx="));
    }

    #[test]
    fn circle_finders_produce_circle_elements() {
        let qr = make_qr();
        let style = QrStyle {
            finder_shape: FinderShape::Circle,
            module_shape: ModuleShape::Square,
            ..Default::default()
        };
        let svg = render_svg(&qr.qr, &style).unwrap();
        let g_start = svg.find("<g>").unwrap();
        let g_end = svg.rfind("</g>").unwrap() + 4;
        let finder_section = &svg[g_start..g_end];
        let circle_count = finder_section.matches("<circle").count();
        assert_eq!(
            circle_count, 9,
            "expected 9 finder circles, got {circle_count}"
        );
    }

    #[test]
    fn custom_colors_appear_in_svg() {
        let qr = make_qr();
        let style = QrStyle {
            fg_color: Color::Hex("#1a1a2e".into()),
            bg_color: Color::Hex("#e0e0e0".into()),
            ..Default::default()
        };
        let svg = render_svg(&qr.qr, &style).unwrap();
        assert!(svg.contains("#1a1a2e"));
        assert!(svg.contains("#e0e0e0"));
    }

    #[test]
    fn rgb_colors_work() {
        let qr = make_qr();
        let style = QrStyle {
            fg_color: Color::Rgb(255, 0, 0),
            bg_color: Color::Rgb(0, 0, 255),
            ..Default::default()
        };
        let svg = render_svg(&qr.qr, &style).unwrap();
        assert!(svg.contains("#ff0000"));
        assert!(svg.contains("#0000ff"));
    }

    #[test]
    fn invalid_color_returns_error() {
        let qr = make_qr();
        let style = QrStyle {
            fg_color: Color::Hex("not-a-color".into()),
            ..Default::default()
        };
        let err = render_svg(&qr.qr, &style).unwrap_err();
        assert_eq!(err, QrError::InvalidColor("not-a-color".into()));
    }

    #[test]
    fn viewbox_accounts_for_quiet_zone() {
        let qr = make_qr();
        let style = QrStyle {
            module_size: 10,
            quiet_zone: 4,
            ..Default::default()
        };
        let svg = render_svg(&qr.qr, &style).unwrap();
        let expected_total = (qr.size() as f64 * 10.0 + 2.0 * 4.0 * 10.0) as u32;
        let viewbox = format!("viewBox=\"0 0 {0} {0}\"", expected_total);
        assert!(svg.contains(&viewbox), "expected {viewbox} in SVG");
    }

    #[test]
    fn radius_above_half_is_clamped() {
        let qr = make_qr();
        let style = QrStyle {
            module_shape: ModuleShape::RoundedSquare { radius: 0.8 },
            ..Default::default()
        };
        // Should not error — just clamps
        let svg = render_svg(&qr.qr, &style).unwrap();
        assert!(svg.contains("rx="));
    }

    #[test]
    fn negative_radius_is_clamped_to_zero() {
        let qr = make_qr();
        let style = QrStyle {
            module_shape: ModuleShape::RoundedSquare { radius: -1.0 },
            ..Default::default()
        };
        let svg = render_svg(&qr.qr, &style).unwrap();
        assert!(svg.contains("rx="));
    }

    #[test]
    fn finder_region_detects_top_left() {
        assert!(is_finder_region(0, 0, 25));
        assert!(is_finder_region(6, 6, 25));
        assert!(!is_finder_region(7, 0, 25));
        assert!(!is_finder_region(0, 7, 25));
    }

    #[test]
    fn finder_region_detects_top_right() {
        assert!(is_finder_region(0, 18, 25));
        assert!(is_finder_region(6, 24, 25));
        assert!(!is_finder_region(7, 18, 25));
    }

    #[test]
    fn finder_region_detects_bottom_left() {
        assert!(is_finder_region(18, 0, 25));
        assert!(is_finder_region(24, 6, 25));
        assert!(!is_finder_region(18, 7, 25));
    }

    #[test]
    fn non_finder_region() {
        assert!(!is_finder_region(10, 10, 25));
        assert!(!is_finder_region(7, 7, 25));
    }
}
