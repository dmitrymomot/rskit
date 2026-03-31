# modo::email

Transactional email with Markdown templates, SMTP delivery, and optional LRU caching.

Requires feature `"email"`.

```toml
[dependencies]
modo = { version = "0.3", features = ["email"] }
```

`Mailer::with_stub_transport` is available with the `test-helpers` feature or in `#[cfg(test)]` blocks.

## Key types

| Type | Description |
| -------------------------------- | -------------------------------------------------- |
| `Mailer` | Renders templates and delivers email over SMTP |
| `EmailConfig` | Top-level configuration (deserializes from YAML) |
| `SmtpConfig` / `SmtpSecurity` | SMTP connection settings and TLS mode |
| `SendEmail` | Builder for composing an outgoing email |
| `SenderProfile` | Per-message `From` / `Reply-To` override |
| `RenderedEmail` | Output of `Mailer::render` (subject, HTML, text) |
| `TemplateSource` | Trait for pluggable template loaders |
| `FileSource` / `CachedSource<S>` | Filesystem loader and LRU-caching wrapper |
| `ButtonType` | Button colour variants (`Primary`, `Danger`, etc.) |

## Usage

### Basic example

```rust,no_run
use modo::email::{EmailConfig, Mailer, SendEmail};

#[tokio::main]
async fn main() -> modo::Result<()> {
    let mailer = Mailer::new(&EmailConfig {
        templates_path: "emails".into(),
        default_from_email: "noreply@example.com".into(),
        ..Default::default()
    })?;

    mailer.send(
        SendEmail::new("welcome", "user@example.com")
            .var("name", "Dmytro"),
    ).await?;
    Ok(())
}
```

### Template format

Markdown files with YAML frontmatter stored under `EmailConfig::templates_path`:

```text
---
subject: Welcome to {{app_name}}!
layout: base
---

Hi {{name}},

[button|Get started](https://example.com/start)
[button:danger|Delete account](https://example.com/delete)
```

`layout` defaults to `"base"` (built-in responsive HTML layout with dark-mode support).
Custom layouts are `.html` files in `EmailConfig::layouts_path`.

Locale fallback: `{locale}/{name}.md` -> `{default_locale}/{name}.md` -> `{name}.md`.

### Button types

| Syntax                         | Colour                              |
| ------------------------------ | ----------------------------------- |
| `[button\|Label](url)`         | Primary (`brand_color` var or blue) |
| `[button:danger\|Label](url)`  | Red                                 |
| `[button:warning\|Label](url)` | Amber                               |
| `[button:info\|Label](url)`    | Cyan                                |
| `[button:success\|Label](url)` | Green                               |

### Custom sender per message

```rust,no_run
use modo::email::{SendEmail, SenderProfile};

let email = SendEmail::new("invoice", "customer@example.com")
    .sender(SenderProfile {
        from_name: "Billing".into(),
        from_email: "billing@example.com".into(),
        reply_to: Some("support@example.com".into()),
    });
```

### Render without sending

```rust,no_run
use modo::email::{EmailConfig, Mailer, SendEmail};

fn example(mailer: &Mailer) -> modo::Result<()> {
    let rendered = mailer.render(&SendEmail::new("welcome", "user@example.com"))?;
    println!("{}", rendered.subject);
    Ok(())
}
```

### Custom template source

```rust,no_run
use modo::email::{EmailConfig, Mailer, TemplateSource};
use modo::Result;
use std::sync::Arc;

struct DbSource;
impl TemplateSource for DbSource {
    fn load(&self, name: &str, _locale: &str, _default_locale: &str) -> Result<String> {
        Ok(format!("---\nsubject: {name}\n---\nBody"))
    }
}
fn build(config: &EmailConfig) -> Result<Mailer> {
    Mailer::with_source(config, Arc::new(DbSource))
}
```

## Configuration

```yaml
email:
    templates_path: emails
    layouts_path: emails/layouts
    default_from_name: My App
    default_from_email: noreply@example.com
    default_locale: en
    cache_templates: true
    template_cache_size: 100
    smtp:
        host: smtp.example.com
        port: 587
        username: user
        password: secret
        security: starttls # starttls | tls | none
```

## Error handling

| Condition | HTTP status | When |
| --- | --- | --- |
| Missing frontmatter | 400 Bad Request | Template lacks `---` delimiters or `subject` field |
| Invalid address | 400 Bad Request | Malformed `To`, `Cc`, `Bcc`, `From`, or `Reply-To` address |
| No recipients | 400 Bad Request | `SendEmail::to` list is empty at send time |
| SMTP auth mismatch | 400 Bad Request | Only one of `username`/`password` is set |
| Template not found | 404 Not Found | Template file missing for the given name and locale |
| Layout not found | 404 Not Found | Requested layout name not in built-in or custom layouts |
| SMTP transport error | 500 Internal | Failed to build or connect to the SMTP server |
| SMTP delivery error | 500 Internal | Server accepted connection but rejected the message |
| Frontmatter parse error | 500 Internal | YAML in frontmatter is syntactically invalid |
