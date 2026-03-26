use minijinja::Value;

use crate::qrcode::code::QrCode;
use crate::qrcode::style::{Color, FinderShape, ModuleShape, QrStyle};

/// Creates a MiniJinja template function that generates QR code SVGs.
///
/// Register it with the template engine:
///
/// ```rust,ignore
/// engine.add_function("qr_svg", modo::qrcode::qr_svg_function());
/// ```
///
/// Template usage:
///
/// ```jinja
/// {{ qr_svg(data="otpauth://totp/...", fg="#1a1a2e") }}
/// ```
pub fn qr_svg_function() -> impl Fn(&[Value]) -> Result<Value, minijinja::Error> + Send + Sync + 'static
{
    move |args: &[Value]| {
        let kwargs = match args.first() {
            Some(v) if v.kind() == minijinja::value::ValueKind::Map => v.clone(),
            _ => {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    "qr_svg() requires keyword arguments",
                ));
            }
        };

        // Required: data
        let data: String = kwargs
            .get_attr("data")
            .ok()
            .filter(|v| !v.is_undefined())
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .ok_or_else(|| {
                minijinja::Error::new(
                    minijinja::ErrorKind::MissingArgument,
                    "qr_svg() requires a 'data' argument",
                )
            })?;

        // Optional: colors
        let fg = get_str_attr(&kwargs, "fg").unwrap_or_else(|| "#000000".into());
        let bg = get_str_attr(&kwargs, "bg").unwrap_or_else(|| "#ffffff".into());

        // Optional: shapes
        let module_shape_str = get_str_attr(&kwargs, "module_shape").unwrap_or_else(|| "rounded".into());
        let finder_shape_str = get_str_attr(&kwargs, "finder_shape").unwrap_or_else(|| "rounded".into());

        // Optional: radius
        let radius = get_f64_attr(&kwargs, "radius").unwrap_or(0.3) as f32;

        // Optional: size
        let size = get_u32_attr(&kwargs, "size").unwrap_or(10);

        let module_shape = parse_module_shape(&module_shape_str, radius)?;
        let finder_shape = parse_finder_shape(&finder_shape_str)?;

        let style = QrStyle {
            module_shape,
            finder_shape,
            fg_color: Color::Hex(fg),
            bg_color: Color::Hex(bg),
            module_size: size,
            quiet_zone: 4,
        };

        let qr = QrCode::new(&data).map_err(|e| {
            minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
        })?;

        let svg = qr.to_svg(&style).map_err(|e| {
            minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
        })?;

        Ok(Value::from_safe_string(svg))
    }
}

fn get_str_attr(kwargs: &Value, key: &str) -> Option<String> {
    kwargs
        .get_attr(key)
        .ok()
        .filter(|v| !v.is_undefined())
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}

fn get_f64_attr(kwargs: &Value, key: &str) -> Option<f64> {
    kwargs
        .get_attr(key)
        .ok()
        .filter(|v| !v.is_undefined())
        .and_then(|v| f64::try_from(v).ok())
}

fn get_u32_attr(kwargs: &Value, key: &str) -> Option<u32> {
    get_f64_attr(kwargs, key).map(|v| v as u32)
}

fn parse_module_shape(s: &str, radius: f32) -> Result<ModuleShape, minijinja::Error> {
    match s {
        "square" => Ok(ModuleShape::Square),
        "rounded" => Ok(ModuleShape::RoundedSquare { radius }),
        "circle" => Ok(ModuleShape::Circle),
        "diamond" => Ok(ModuleShape::Diamond),
        other => Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("unknown module_shape: '{other}' (expected: square, rounded, circle, diamond)"),
        )),
    }
}

fn parse_finder_shape(s: &str) -> Result<FinderShape, minijinja::Error> {
    match s {
        "square" => Ok(FinderShape::Square),
        "rounded" => Ok(FinderShape::Rounded),
        "circle" => Ok(FinderShape::Circle),
        other => Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("unknown finder_shape: '{other}' (expected: square, rounded, circle)"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use minijinja::Environment;

    use super::*;

    fn render_template(template: &str) -> Result<String, minijinja::Error> {
        let mut env = Environment::new();
        env.add_function("qr_svg", qr_svg_function());
        let tmpl = env.template_from_str(template)?;
        tmpl.render(minijinja::context! {})
    }

    #[test]
    fn basic_qr_svg_call() {
        let result = render_template(r#"{{ qr_svg(data="test") }}"#).unwrap();
        assert!(result.contains("<svg"));
        assert!(result.contains("</svg>"));
    }

    #[test]
    fn custom_colors() {
        let result =
            render_template(r##"{{ qr_svg(data="test", fg="#ff0000", bg="#00ff00") }}"##).unwrap();
        assert!(result.contains("#ff0000"));
        assert!(result.contains("#00ff00"));
    }

    #[test]
    fn custom_module_shape() {
        let result =
            render_template(r#"{{ qr_svg(data="test", module_shape="circle") }}"#).unwrap();
        assert!(result.contains("<circle"));
    }

    #[test]
    fn custom_finder_shape_circle() {
        let result =
            render_template(r#"{{ qr_svg(data="test", finder_shape="circle", module_shape="square") }}"#)
                .unwrap();
        assert!(result.contains("<circle"));
    }

    #[test]
    fn missing_data_is_error() {
        let result = render_template(r##"{{ qr_svg(fg="#000") }}"##);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_module_shape_is_error() {
        let result = render_template(r#"{{ qr_svg(data="test", module_shape="star") }}"#);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_finder_shape_is_error() {
        let result = render_template(r#"{{ qr_svg(data="test", finder_shape="triangle") }}"#);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_color_is_error() {
        let result = render_template(r##"{{ qr_svg(data="test", fg="not-a-color") }}"##);
        assert!(result.is_err());
    }

    #[test]
    fn defaults_produce_rounded_svg() {
        let result = render_template(r#"{{ qr_svg(data="test") }}"#).unwrap();
        assert!(result.contains("rx="));
    }

    #[test]
    fn custom_size() {
        let result = render_template(r#"{{ qr_svg(data="A", size=20) }}"#).unwrap();
        // Version 1 QR = 21 modules, quiet zone 4, module_size 20
        // total = (21 + 8) * 20 = 580
        assert!(result.contains("viewBox=\"0 0 580 580\""));
    }
}
