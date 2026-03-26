# QR Code Generator — Design Spec

**Date:** 2026-03-26
**Module:** `src/qrcode/`
**Feature flag:** `qrcode`
**Dependency:** `fast_qr` (QR matrix generation)

## Overview

Standalone QR code generator module for modo. Produces customizable SVG output from any string input. Primary use case is TOTP authenticator enrollment (Google Authenticator, Apple Passwords, etc.) via `otpauth://` URIs, but the module is general-purpose.

The existing `Totp::otpauth_uri()` in `src/auth/totp.rs` already generates the URI — this module turns it into a scannable QR code image.

## Module Structure

```
src/qrcode/
  mod.rs        — mod imports and re-exports only
  code.rs       — QrCode struct, matrix generation via fast_qr
  style.rs      — QrStyle, ModuleShape, FinderShape, Color types
  render.rs     — SVG renderer (matrix + style → SVG string)
  template.rs   — MiniJinja template function
  error.rs      — QrError enum
```

Feature-gated under `qrcode` in `Cargo.toml`:
```toml
qrcode = ["dep:fast_qr"]
```

## Public API

### QrCode

```rust
pub struct QrCode {
    // internal: matrix from fast_qr, version info
}

impl QrCode {
    /// Generate QR matrix with default error correction (Medium).
    pub fn new(data: &str) -> Result<Self, QrError>;

    /// Generate QR matrix with specified error correction level.
    pub fn with_ecl(data: &str, ecl: Ecl) -> Result<Self, QrError>;

    /// Render the QR code as an SVG string.
    pub fn to_svg(&self, style: &QrStyle) -> String;
}
```

`Ecl` is modo's own enum (`Low`, `Medium`, `Quartile`, `High`) mapped to `fast_qr` equivalents internally — the dependency does not leak into the public API. Default is `Medium`.

### QrStyle

```rust
pub struct QrStyle {
    pub module_shape: ModuleShape,
    pub finder_shape: FinderShape,
    pub fg_color: Color,
    pub bg_color: Color,
    pub module_size: u32,
    pub quiet_zone: u32,
}
```

**Default values:** `ModuleShape::RoundedSquare { radius: 0.3 }`, `FinderShape::Rounded`, black (`#000000`) on white (`#ffffff`), 10px module size, 4-module quiet zone.

All fields are public for struct literal construction.

### ModuleShape

```rust
pub enum ModuleShape {
    Square,
    RoundedSquare { radius: f32 },  // 0.0–0.5, fraction of module size; clamped at render time
    Circle,
    Diamond,
}
```

### FinderShape

```rust
pub enum FinderShape {
    Square,
    Rounded,
    Circle,
}
```

### Color

```rust
pub enum Color {
    Hex(String),
    Rgb(u8, u8, u8),
}
```

`Hex` is validated at render time — invalid values produce `QrError::InvalidColor`. Supports both 3-char (`#fff`) and 6-char (`#ffffff`) shorthand; 3-char is expanded internally. `Rgb` is valid by construction.

## SVG Rendering

### Module rendering (data cells)

Each module shape maps to an SVG element:

- **Square** — `<rect>` elements
- **RoundedSquare** — `<rect rx="..." ry="...">` with radius = fraction x module_size
- **Circle** — `<circle>` centered in each module cell
- **Diamond** — `<polygon>` with 4 points forming a 45-degree rotated square

### Finder rendering (corner patterns)

The three 7x7 finder patterns (top-left, top-right, bottom-left) are detected by position and rendered as grouped units, not cell-by-cell:

- **Square** — three concentric `<rect>` elements (7x7 outer, 5x5 gap, 3x3 inner)
- **Rounded** — same structure with `rx`/`ry` on each rect
- **Circle** — three concentric `<circle>` elements

### SVG structure

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}">
  <rect fill="{bg}" width="100%" height="100%"/>
  <!-- finder patterns as grouped elements -->
  <g>...</g>
  <!-- data modules, excluding finder regions -->
  <rect ... /> or <circle ... />
</svg>
```

Uses `viewBox` for resolution independence — no inline `width`/`height` so the consumer controls sizing via CSS or HTML attributes.

Output is a `String`. No `Value::from_safe_string()` wrapping at this layer.

## Template Function

A standalone MiniJinja function the app registers:

```rust
pub fn qr_svg_function() -> impl Function;
```

**File:** `src/qrcode/template.rs`

**Template usage:**
```jinja
{{ qr_svg(data="otpauth://totp/...", fg="#1a1a2e", bg="#ffffff") }}
```

**Parameters:**

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `data` | yes | — | String to encode |
| `fg` | no | `"#000000"` | Foreground hex color |
| `bg` | no | `"#ffffff"` | Background hex color |
| `module_shape` | no | `"rounded"` | `"square"`, `"rounded"`, `"circle"`, `"diamond"` |
| `finder_shape` | no | `"rounded"` | `"square"`, `"rounded"`, `"circle"` |
| `radius` | no | `0.3` | Corner radius for `"rounded"` module shape (0.0–0.5) |
| `size` | no | `10` | Module size in px |

Returns `Value::from_safe_string(svg)` for raw HTML rendering. Errors produce a MiniJinja error.

**Registration is the app's responsibility:**
```rust
engine.add_function("qr_svg", modo::qrcode::qr_svg_function());
```

## Error Handling

```rust
pub enum QrError {
    DataTooLong,
    InvalidColor(String),
}
```

- `DataTooLong` — input exceeds QR code capacity for the chosen ECL
- `InvalidColor(String)` — invalid hex color string (missing `#`, wrong length, non-hex chars)

Implements `Display`, `std::error::Error`, and `Into<modo::Error>` (maps to `Error::bad_request()`).

`fast_qr` errors are mapped to `DataTooLong` internally — the dependency does not leak into the public API.

## Testing Strategy

**Unit tests (~15-20):**

- **Matrix generation:** valid data produces correct-size matrix, empty string works, oversized data returns `DataTooLong`
- **Color parsing:** valid hex (`#000000`, `#fff`), invalid hex (`"red"`, `"#gggggg"`), RGB construction
- **SVG output:** default style produces valid SVG, each `ModuleShape` variant renders expected SVG elements, each `FinderShape` variant renders expected elements
- **Finder detection:** finder regions correctly identified and excluded from data module rendering
- **Style defaults:** `QrStyle::default()` produces rounded shapes, black/white, 10px, 4-module quiet zone
- **Template function:** valid params produce SVG string, missing `data` errors, invalid color errors, optional params use defaults

**Verification approach:** Tests assert SVG contains expected elements (`<rect rx=`, `<circle`, `viewBox`, color values) rather than exact string matching. This keeps tests resilient to minor rendering tweaks.

**No integration tests needed** — this is pure computation with no I/O.

## Dependencies

```toml
[dependencies]
fast_qr = { version = "0.13", optional = true }
```

No other new dependencies. SVG is generated via string building (`String` / `write!`).
