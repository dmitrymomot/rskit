# QR Code Module

QR code generation with customizable SVG output, feature-gated under `qrcode`.

## Feature Flag

Enable in `Cargo.toml`:

```toml
modo = { version = "0.3", features = ["qrcode"] }
```

For template integration (MiniJinja), also enable `templates`:

```toml
modo = { version = "0.3", features = ["qrcode", "templates"] }
```

## Key Types

| Type              | Purpose                                                                                                              |
| ----------------- | -------------------------------------------------------------------------------------------------------------------- |
| `QrCode`          | Generated QR matrix. Create with `QrCode::new(data)` or `QrCode::with_ecl(data, ecl)`, render with `to_svg(&style)`. |
| `QrStyle`         | SVG rendering options: module shape, finder shape, foreground/background colors, module size, quiet zone.            |
| `Ecl`             | Error correction level: `Low` (~7%), `Medium` (~15%, default), `Quartile` (~25%), `High` (~30%).                     |
| `Color`           | Color value as `Color::Hex("#ff0000")` or `Color::Rgb(255, 0, 0)`.                                                   |
| `ModuleShape`     | Shape of data modules: `Square`, `RoundedSquare { radius }`, `Circle`, `Diamond`.                                    |
| `FinderShape`     | Shape of the three 7x7 finder patterns: `Square`, `Rounded`, `Circle`.                                               |
| `QrError`         | Error type with variants `DataTooLong` and `InvalidColor`. Converts to `modo::Error` (HTTP 400).                     |
| `qr_svg_function` | MiniJinja template function factory (requires `templates` feature).                                                  |

## Usage

### Basic

```rust
use modo::qrcode::{QrCode, QrStyle};

let qr = QrCode::new("https://example.com").unwrap();
let svg = qr.to_svg(&QrStyle::default()).unwrap();
// svg is a complete <svg> element with viewBox (no fixed width/height)
```

### Custom Error Correction

```rust
use modo::qrcode::{QrCode, QrStyle, Ecl};

let qr = QrCode::with_ecl("otpauth://totp/Example:user@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example", Ecl::High).unwrap();
let svg = qr.to_svg(&QrStyle::default()).unwrap();
```

### Custom Styling

```rust
use modo::qrcode::{QrCode, QrStyle, ModuleShape, FinderShape, Color};

let style = QrStyle {
    module_shape: ModuleShape::Circle,
    finder_shape: FinderShape::Circle,
    fg_color: Color::Rgb(26, 26, 46),
    bg_color: Color::Hex("#f0f0f0".into()),
    module_size: 12,
    quiet_zone: 4,
};

let qr = QrCode::new("https://example.com").unwrap();
let svg = qr.to_svg(&style).unwrap();
```

### Template Integration

Register the template function with MiniJinja:

```rust,ignore
engine.add_function("qr_svg", modo::qrcode::qr_svg_function());
```

Use in templates:

```jinja
{{ qr_svg(data="otpauth://totp/...", fg="#1a1a2e") }}
```

Available keyword arguments:

| Argument       | Required | Default     | Description                                           |
| -------------- | -------- | ----------- | ----------------------------------------------------- |
| `data`         | yes      | --          | String to encode                                      |
| `fg`           | no       | `"#000000"` | Foreground color (hex)                                |
| `bg`           | no       | `"#ffffff"` | Background color (hex)                                |
| `module_shape` | no       | `"rounded"` | `"square"`, `"rounded"`, `"circle"`, or `"diamond"`   |
| `finder_shape` | no       | `"rounded"` | `"square"`, `"rounded"`, or `"circle"`                |
| `radius`       | no       | `0.3`       | Corner radius for `"rounded"` module shape (0.0--0.5) |
| `size`         | no       | `10`        | Module size in SVG units                              |

## Customization Options

### Module Shapes (4 options)

- **Square** -- classic sharp-edged square modules
- **RoundedSquare { radius }** -- rounded corners; `radius` is a fraction of module size (0.0 = square, 0.5 = maximum rounding), clamped at render time
- **Circle** -- circular dot modules
- **Diamond** -- 45-degree rotated square modules

### Finder Shapes (3 options)

- **Square** -- concentric sharp-edged squares
- **Rounded** -- concentric rounded rectangles
- **Circle** -- concentric circles

### Colors

Colors can be specified as:

- `Color::Hex("#ff0000")` -- 6-digit hex with `#` prefix
- `Color::Hex("#f00")` -- 3-digit shorthand (expanded to 6-digit)
- `Color::Rgb(255, 0, 0)` -- RGB tuple (each component 0--255)

### Default Style

The `QrStyle::default()` produces:

- Module shape: `RoundedSquare { radius: 0.3 }`
- Finder shape: `Rounded`
- Foreground: black (`#000000`)
- Background: white (`#ffffff`)
- Module size: 10 SVG units
- Quiet zone: 4 modules (spec-recommended minimum)

## SVG Output

The rendered SVG:

- Uses `viewBox` only (no fixed `width`/`height`), so it scales to its container
- Includes a full background rectangle
- Renders finder patterns as grouped (`<g>`) concentric shapes
- Skips light (background) modules for smaller output

## Error Handling

`QrError` has two variants:

- `DataTooLong` -- input data exceeds QR capacity for the chosen ECL
- `InvalidColor(String)` -- malformed hex color (missing `#`, wrong length, non-hex characters)

Both convert to `modo::Error` with HTTP 400 status and a stable error code (`"qrcode:data_too_long"` or `"qrcode:invalid_color"`).

## Dependencies

Uses `fast_qr` 0.13 for QR matrix generation (optional dependency, pulled in by the `qrcode` feature flag).
