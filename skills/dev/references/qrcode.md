# QR Code Generation (`modo::qrcode`)

Always available.

Source: `src/qrcode/`.

## Public API

Import from `modo::qrcode`:

```rust
use modo::qrcode::{Color, Ecl, FinderShape, ModuleShape, QrCode, QrError, QrStyle};
```

The MiniJinja template helper is also exposed:

```rust
use modo::qrcode::qr_svg_function;
```

---

## Ecl

Error correction level for QR code generation. Higher levels increase data recovery at the cost of larger QR codes.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecl {
    Low,       // recovers ~7% of data
    Medium,    // recovers ~15% of data (default)
    Quartile,  // recovers ~25% of data
    High,      // recovers ~30% of data
}
```

`QrCode::new` defaults to `Ecl::Medium`.

---

## QrCode

A generated QR code matrix ready for SVG rendering.

```rust
#[derive(Debug)]
pub struct QrCode {
    pub(super) qr: fast_qr::QRCode,
}
```

### new(data: &str) -> Result<Self, QrError>

Generate a QR code matrix with default error correction (`Ecl::Medium`). Returns `QrError::DataTooLong` if the input exceeds QR capacity.

```rust
let qr = QrCode::new("https://example.com")?;
```

### with_ecl(data: &str, ecl: Ecl) -> Result<Self, QrError>

Generate a QR code matrix with the specified error correction level. Returns `QrError::DataTooLong` if the input exceeds QR capacity for the chosen level.

```rust
let qr = QrCode::with_ecl("https://example.com", Ecl::High)?;
```

### to_svg(&self, style: &QrStyle) -> Result<String, QrError>

Render the QR code as an SVG string. The SVG uses a `viewBox` (no fixed `width`/`height`) so it scales to its container. Returns `QrError::InvalidColor` if any color in `style` is malformed.

```rust
let svg = qr.to_svg(&QrStyle::default())?;
assert!(svg.starts_with("<svg"));
```

### size(&self) -> usize

Returns the number of modules along one side of the QR matrix. This is the raw matrix dimension (e.g. 21 for Version 1) and does not include the quiet zone added during SVG rendering.

---

## QrError

Errors that can occur during QR code generation or rendering.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QrError {
    DataTooLong,
    InvalidColor(String),
}
```

### code(&self) -> &'static str

Returns a stable, namespaced string code for this error:

| Variant          | Code                     |
| ---------------- | ------------------------ |
| `DataTooLong`    | `"qrcode:data_too_long"` |
| `InvalidColor(_)`| `"qrcode:invalid_color"` |

### Trait implementations

- `Display` -- human-readable error message
- `std::error::Error`
- `From<QrError> for modo::Error` -- converts to `Error::bad_request` with the error code attached via `.with_code()`

---

## ModuleShape

Shape of individual data modules (the small squares/dots).

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ModuleShape {
    Square,
    RoundedSquare { radius: f32 },  // radius is fraction of module size (0.0..=0.5)
    Circle,
    Diamond,
}
```

Default: `RoundedSquare { radius: 0.3 }`.

The `radius` field is clamped to `[0.0, 0.5]` during rendering.

---

## FinderShape

Shape of the three finder patterns (the large 7x7 corner markers).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinderShape {
    Square,
    Rounded,
    Circle,
}
```

Default: `Rounded`.

---

## Color

A color value for QR code rendering.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Color {
    Hex(String),         // "#000000" (6-digit) or "#000" (3-digit shorthand)
    Rgb(u8, u8, u8),     // RGB components, each 0--255
}
```

### to_hex(&self) -> Result<String, QrError>

Resolves the color to a lowercase hex string with `#` prefix (e.g. `"#1a1a2e"`). Three-digit shorthand is expanded (`"#fff"` becomes `"#ffffff"`). Returns `QrError::InvalidColor` if the hex value is malformed (missing `#`, wrong length, or non-hex characters).

### Trait implementations

- `Display` -- displays color as hex, or `"(invalid)"` if malformed

---

## QrStyle

Styling options for QR code SVG rendering. All fields are public.

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct QrStyle {
    pub module_shape: ModuleShape,  // default: RoundedSquare { radius: 0.3 }
    pub finder_shape: FinderShape,  // default: Rounded
    pub fg_color: Color,            // default: Hex("#000000")
    pub bg_color: Color,            // default: Hex("#ffffff")
    pub module_size: u32,           // default: 10
    pub quiet_zone: u32,            // default: 4
}
```

`impl Default` provides the defaults listed above.

Example with customization:

```rust
use modo::qrcode::{QrStyle, ModuleShape, FinderShape, Color};

let style = QrStyle {
    module_shape: ModuleShape::Circle,
    finder_shape: FinderShape::Circle,
    fg_color: Color::Rgb(26, 26, 46),
    ..Default::default()
};
```

---

## qr_svg_function

```rust
pub fn qr_svg_function() -> impl Fn(&[Value]) -> Result<Value, minijinja::Error> + Send + Sync + 'static
```

Creates a MiniJinja template function that generates QR code SVGs. Register with the template engine:

```rust
engine.add_function("qr_svg", modo::qrcode::qr_svg_function());
```

### Template usage

```jinja
{{ qr_svg(data="otpauth://totp/...", fg="#1a1a2e") }}
```

### Keyword arguments

| Argument       | Required | Default     | Description                                           |
| -------------- | -------- | ----------- | ----------------------------------------------------- |
| `data`         | yes      | --          | The string to encode in the QR code                   |
| `fg`           | no       | `"#000000"` | Foreground color (hex string)                         |
| `bg`           | no       | `"#ffffff"` | Background color (hex string)                         |
| `module_shape` | no       | `"rounded"` | `"square"`, `"rounded"`, `"circle"`, or `"diamond"`   |
| `finder_shape` | no       | `"rounded"` | `"square"`, `"rounded"`, or `"circle"`                |
| `radius`       | no       | `0.3`       | Corner radius for `"rounded"` module shape (0.0--0.5) |
| `size`         | no       | `10`        | Module size in SVG units                              |

The output is marked as safe HTML (`Value::from_safe_string`), so it renders directly without escaping.

### Error handling

- Missing `data` argument: `minijinja::ErrorKind::MissingArgument`
- Invalid shape string: `minijinja::ErrorKind::InvalidOperation`
- Invalid color or data too long: `minijinja::ErrorKind::InvalidOperation`

---

## Example

```rust
use modo::qrcode::{QrCode, QrStyle, Ecl, Color, ModuleShape};

// Basic usage
let qr = QrCode::new("https://example.com").unwrap();
let svg = qr.to_svg(&QrStyle::default()).unwrap();

// Custom style
let style = QrStyle {
    module_shape: ModuleShape::Circle,
    fg_color: Color::Hex("#1a1a2e".into()),
    ..Default::default()
};
let svg = qr.to_svg(&style).unwrap();

// TOTP enrollment
let uri = "otpauth://totp/MyApp:user@example.com?secret=BASE32SECRET&issuer=MyApp";
let qr = QrCode::with_ecl(uri, Ecl::Medium).unwrap();
let svg = qr.to_svg(&QrStyle::default()).unwrap();
```

---

## Gotchas

- **SVG viewBox, no fixed dimensions**: The SVG uses `viewBox` (not fixed `width`/`height`), so it scales to its container. Apply CSS or inline `width`/`height` on the caller side.

- **Quiet zone is part of SVG dimensions**: A Version 1 QR (21 modules) with `module_size=10` and `quiet_zone=4` has a viewBox of `"0 0 290 290"` ((21 + 8) x 10).

- **Color validation is deferred**: `Color::Hex` accepts any string at construction time. Validation happens when `to_hex()` is called (during `to_svg()` rendering). Invalid colors produce `QrError::InvalidColor`.

- **Radius clamping**: `ModuleShape::RoundedSquare { radius }` values outside `[0.0, 0.5]` are silently clamped during rendering.

- **fast_qr error mapping**: All `fast_qr` build errors are mapped to `QrError::DataTooLong` since that is the only realistic failure mode for the builder.

- **Template function quiet_zone**: The template function always uses `quiet_zone=4` (not configurable via template arguments).
