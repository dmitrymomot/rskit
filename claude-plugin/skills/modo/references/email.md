# Email Module

Feature gate: `email` (add `email-test` for stub transport in tests).

Source: `src/email/`.

## Overview

Markdown-based transactional email with SMTP delivery. Templates use YAML frontmatter for metadata, `{{var}}` placeholders for substitution, and a custom `[button|Label](url)` syntax for call-to-action buttons. Rendering produces both HTML (with layout) and plain-text (auto-derived) bodies, sent as `multipart/alternative`.

## Configuration

`EmailConfig` is `#[non_exhaustive]`. Derives `Debug`, `Clone`, `Deserialize`. Has `impl Default` (manual). Deserializes from YAML under the `email` key. All fields have defaults.

```yaml
email:
  templates_path: emails           # directory with .md templates
  layouts_path: emails/layouts     # directory with .html layout files
  default_from_name: MyApp              # default: "" (empty string)
  default_from_email: noreply@example.com  # default: "" (empty string)
  default_reply_to: support@example.com  # optional, default: null
  default_locale: en
  cache_templates: true
  template_cache_size: 100
  smtp:
    host: smtp.example.com  # default: "localhost"
    port: 587
    username: user
    password: pass
    security: starttls             # starttls | tls | none
```

`SmtpConfig` is `#[non_exhaustive]`. Derives `Debug`, `Clone`, `Deserialize`. Has `impl Default` (manual, defaults: host `"localhost"`, port `587`, no credentials, `StartTls` security).

### `SmtpSecurity` variants

`SmtpSecurity` derives `Debug`, `Clone`, `Default`, `Deserialize`, `PartialEq`. YAML values are lowercase (`#[serde(rename_all = "lowercase")]`).

| Variant    | YAML value   | Description                          |
|------------|-------------|--------------------------------------|
| `StartTls` | `starttls`  | STARTTLS upgrade (`#[default]`, port 587) |
| `Tls`      | `tls`       | Implicit TLS (port 465)              |
| `None`     | `none`      | No encryption (dev/local relay only) |

Username and password must both be set or both be absent -- mismatched pair returns an error.

## Template Format

Templates are `.md` files with YAML frontmatter:

```text
---
subject: Welcome to {{app_name}}, {{name}}!
layout: base
---

Hi {{name}},

Thanks for signing up.

[button|Get Started](https://example.com/start?token={{token}})
```

### Frontmatter fields

| Field     | Required | Default | Description                   |
|-----------|----------|---------|-------------------------------|
| `subject` | yes      | --      | Email subject line            |
| `layout`  | no       | `base`  | Layout name (`base` = built-in) |

### Variable substitution

`{{var_name}}` placeholders are replaced in both frontmatter and body before parsing. Variable names must match `[a-zA-Z_][a-zA-Z0-9_]*`. Missing variables become empty strings.

### Button syntax

Inside Markdown links, the link text triggers button rendering:

```text
[button|Label](url)              -> Primary (blue / brand_color)
[button:primary|Label](url)      -> Primary
[button:danger|Label](url)       -> Red
[button:warning|Label](url)      -> Amber
[button:info|Label](url)         -> Cyan
[button:success|Label](url)      -> Green
```

If a `brand_color` template variable is set, it overrides the primary button color. Other button types are unaffected.

HTML output uses a table-based layout for Outlook compatibility. Plain-text output renders as `Label: url`.

Unrecognized button types (e.g., `[button:unknown|X](url)`) render as a normal link.

## Layouts

The built-in `base` layout provides:
- 600px max-width, responsive (mobile collapses to full width)
- Dark mode support (`prefers-color-scheme: dark`)
- System font stack
- MSO conditional comments for Outlook

### Conditional sections

These template variables activate optional layout sections:
- `logo_url` -- renders a centered logo image above the content card
- `footer_text` -- renders a footer row below the content card

### Custom layouts

Place `.html` files in the `layouts_path` directory. The file stem becomes the layout name (e.g., `marketing.html` -> `layout: marketing`). Custom layouts must contain a `{{content}}` placeholder and can use any `{{var}}` placeholders.

## Template Source

`TemplateSource` trait (object-safe, `Send + Sync`):

```rust
pub trait TemplateSource: Send + Sync {
    fn load(&self, name: &str, locale: &str, default_locale: &str) -> Result<String>;
}
```

### `FileSource`

Constructor: `FileSource::new(templates_path: impl Into<PathBuf>) -> Self`.

Loads `.md` files from disk with locale fallback chain:
1. `{templates_path}/{locale}/{name}.md`
2. `{templates_path}/{default_locale}/{name}.md`
3. `{templates_path}/{name}.md`
4. Error

### `CachedSource<S: TemplateSource>`

Constructor: `CachedSource::new(inner: S, capacity: usize) -> Self` (a capacity of `0` is treated as `1`).

LRU-cached wrapper around any `TemplateSource`. Implements `TemplateSource`. Cache key is the `(name, locale, default_locale)` triple. Enabled by default via `cache_templates: true`.

## Mailer

`Mailer` is the primary entry point. It holds the template source, SMTP transport, config, and preloaded layouts.

### Construction

```rust
// Default file source (optionally cached based on config)
let mailer = Mailer::new(&email_config)?;

// Custom source
let source: Arc<dyn TemplateSource> = Arc::new(MyDbSource::new());
let mailer = Mailer::with_source(&email_config, source)?;

// Stub transport for tests (requires feature "email-test")
let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
let mailer = Mailer::with_stub_transport(&email_config, stub)?;
```

### Rendering without sending

```rust
let email = SendEmail::new("welcome", "user@example.com")
    .var("name", "Dmytro")
    .var("app_name", "Modo");

let rendered: RenderedEmail = mailer.render(&email)?;
// rendered.subject, rendered.html, rendered.text
```

### Sending

```rust
let email = SendEmail::new("welcome", "user@example.com")
    .to("another@example.com")      // additional To
    .cc("cc@example.com")
    .bcc("bcc@example.com")
    .locale("uk")                    // override default locale
    .var("name", "Dmytro")
    .var("brand_color", "#7c3aed")
    .var("logo_url", "https://example.com/logo.png")
    .var("footer_text", "Copyright 2026 Modo Inc.")
    .sender(SenderProfile {
        from_name: "Support".into(),
        from_email: "support@example.com".into(),
        reply_to: Some("help@example.com".into()),
    });

mailer.send(email).await?;
```

`send()` calls `render()` internally, builds a `multipart/alternative` message (text/plain + text/html), and delivers via the configured SMTP transport.

Errors: empty recipient list, malformed addresses, SMTP delivery failures.

## Rendering Pipeline

1. **Load** -- `TemplateSource::load()` fetches raw template with locale fallback
2. **Substitute** -- `{{var}}` placeholders replaced in the entire template string
3. **Parse frontmatter** -- YAML block extracted, `subject` and `layout` read
4. **Markdown to HTML** -- `pulldown-cmark` converts body; button syntax intercepted and rendered as table-based HTML
5. **Apply layout** -- Layout HTML wraps the rendered body; `{{content}}`, `{{logo_section}}`, `{{footer_section}}`, and any `{{var}}` placeholders resolved
6. **Plain text** -- Markdown converted to plain text (links become `text (url)`, buttons become `Label: url`)

## Public API Summary

| Type             | Description                                      |
|------------------|--------------------------------------------------|
| `Mailer`         | Renders templates and sends email over SMTP      |
| `SendEmail`      | Builder for composing an email to send            |
| `RenderedEmail`  | Result of rendering: `subject`, `html`, `text`    |
| `SenderProfile`  | Per-email From/Reply-To override                  |
| `EmailConfig`    | Top-level config (templates, defaults, SMTP)      |
| `SmtpConfig`     | SMTP host, port, credentials, security mode       |
| `SmtpSecurity`   | `StartTls` / `Tls` / `None`                       |
| `TemplateSource` | Trait for loading raw templates                   |
| `FileSource`     | Filesystem template source with locale fallback   |
| `CachedSource<S: TemplateSource>` | LRU-cached wrapper for any `TemplateSource`. `impl TemplateSource` |
| `ButtonType`     | Enum: `Primary`, `Danger`, `Warning`, `Info`, `Success`. Derives `Debug`, `Clone`, `Copy`, `PartialEq` |

## Dependencies

- `lettre` 0.11 -- SMTP transport and message building (with `tokio1-rustls` TLS)
- `pulldown-cmark` 0.13 -- Markdown to HTML conversion
