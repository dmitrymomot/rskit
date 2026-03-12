# modo-email

Email sending for the [modo](https://github.com/dmitrymomot/modo) framework. Markdown templates, responsive HTML output, pluggable transports, multi-tenant sender customization.

## Features

- **Markdown templates** with YAML frontmatter and `{{var}}` substitution
- **Button syntax** -- `[button|Label](url)` renders as email-safe table-based buttons
- **Responsive HTML layout** with dark mode support, or bring your own
- **Plain text fallback** auto-generated from Markdown
- **Transports** -- SMTP (default) and Resend HTTP API
- **Multi-tenant** -- per-email `SenderProfile` override and brand context variables
- **Serializable payload** (`SendEmailPayload`) for async sending via modo-jobs
- **Custom template sources** -- implement `TemplateProvider` for DB-backed templates, APIs, etc.

## Quick Start

Add to `Cargo.toml`:

```toml
[dependencies]
modo-email = { path = "../modo-email" }

# For Resend instead of (or alongside) SMTP:
# modo-email = { path = "../modo-email", features = ["resend"] }
```

### Configuration

```yaml
# config.yaml (or however your app loads config)
email:
    transport: smtp # or "resend"
    templates_path: "emails"
    default_from_name: "My App"
    default_from_email: "hello@myapp.com"
    default_reply_to: "support@myapp.com"
    smtp:
        host: "smtp.example.com"
        port: 587
        username: "user"
        password: "pass"
        tls: true
```

All fields have defaults (`templates_path` defaults to `"emails"`, `smtp.port` to `587`, `smtp.tls` to `true`). Only specify what you need to override.

### Setup and Send

```rust
use modo_email::{mailer, EmailConfig, SendEmail};

// Build the mailer from config (uses FilesystemProvider + configured transport).
let config: EmailConfig = load_config(); // your config loading
let m = mailer(&config)?;

// Send a templated email.
m.send(
    &SendEmail::new("welcome", "user@example.com")
        .var("name", "Alice")
        .var("dashboard_url", "https://app.com/dashboard"),
).await?;
```

### Render Without Sending

```rust
let message = m.render(
    &SendEmail::new("welcome", "user@example.com")
        .var("name", "Alice"),
)?;

println!("Subject: {}", message.subject);
println!("HTML: {}", message.html);
println!("Text: {}", message.text);
```

## Template Format

Templates are `.md` files with YAML frontmatter:

```markdown
---
subject: "Welcome {{name}}!"
layout: default
---

Hi **{{name}}**,

Thanks for joining! Here's what you can do next.

[button|Get Started]({{dashboard_url}})

If you have questions, just reply to this email.
```

### Frontmatter Fields

| Field     | Required | Description                                                          |
| --------- | -------- | -------------------------------------------------------------------- |
| `subject` | Yes      | Email subject line. Supports `{{var}}` placeholders.                 |
| `layout`  | No       | Layout name to wrap the body in. Falls back to `"default"` built-in. |

### Variable Substitution

Use `{{key}}` or `{{ key }}` (whitespace is trimmed). Variables are passed via the builder:

```rust
SendEmail::new("invoice", "user@example.com")
    .var("name", "Alice")
    .var("amount", "$49.00")
    .var("invoice_url", "https://app.com/invoices/123")
```

Unresolved placeholders are left as-is in the output.

### Button Syntax

```markdown
[button|Click Me](https://example.com)
```

Renders as an email-safe, table-based CTA button with a default indigo (`#4F46E5`) background. Customize the color per-email with the `brand_color` variable:

```rust
SendEmail::new("welcome", "user@example.com")
    .var("brand_color", "#E11D48")
```

Regular Markdown links render as styled inline links:

```markdown
[View docs](https://docs.example.com)
```

## Directory Structure

### Single Language

```
emails/
  welcome.md
  reset_password.md
  invoice.md
```

### Multi-Language (Locale Subdirectories)

```
emails/
  welcome.md              # default / fallback
  reset_password.md
  de/
    welcome.md            # German override
    reset_password.md
  fr/
    welcome.md            # French override
  layouts/
    default.html          # custom layout (overrides built-in)
    minimal.html          # additional named layout
```

Locale resolution: if `de/welcome.md` exists, it is used; otherwise falls back to `welcome.md`.

```rust
SendEmail::new("welcome", "user@example.com")
    .locale("de")
    .var("name", "Hans")
```

## Layouts

The built-in `default` layout provides a responsive, dark-mode-aware HTML email wrapper. It supports these context variables:

| Variable       | Description                                 |
| -------------- | ------------------------------------------- |
| `content`      | Rendered HTML body (injected automatically) |
| `subject`      | Email subject (injected automatically)      |
| `logo_url`     | Optional logo image URL                     |
| `product_name` | Optional product name (logo alt text)       |
| `footer_text`  | Optional footer text                        |

To override the built-in layout or add custom ones, place `.html` files in `{templates_path}/layouts/`:

```
emails/layouts/default.html    # overrides built-in default
emails/layouts/minimal.html    # new named layout
```

Layouts use [MiniJinja](https://docs.rs/minijinja) syntax. All email context variables are available. Auto-escaping is disabled since the content is pre-rendered HTML.

```html
<!-- emails/layouts/minimal.html -->
<html>
    <body style="font-family: sans-serif; padding: 20px;">
        {{content}}
    </body>
</html>
```

Reference it in a template's frontmatter:

```markdown
---
subject: "Alert"
layout: minimal
---
```

## Multi-Tenant Usage

### Per-Email Sender Override

```rust
use modo_email::{SendEmail, SenderProfile};

let tenant_sender = SenderProfile {
    from_name: "Tenant Corp".to_string(),
    from_email: "notifications@tenant.com".to_string(),
    reply_to: Some("support@tenant.com".to_string()),
};

m.send(
    &SendEmail::new("welcome", "user@example.com")
        .sender(&tenant_sender)
        .var("name", "Bob"),
).await?;
```

Without `.sender()`, the mailer uses the default sender from `EmailConfig`.

### Brand Context Variables

Pass brand-specific variables to customize templates and layouts per tenant:

```rust
use std::collections::HashMap;

let mut brand = HashMap::new();
brand.insert("logo_url".to_string(), serde_json::json!("https://tenant.com/logo.png"));
brand.insert("product_name".to_string(), serde_json::json!("Tenant Corp"));
brand.insert("brand_color".to_string(), serde_json::json!("#E11D48"));
brand.insert("footer_text".to_string(), serde_json::json!("(c) 2026 Tenant Corp"));

m.send(
    &SendEmail::new("welcome", "user@example.com")
        .context(&brand)
        .var("name", "Alice"),
).await?;
```

## Async Sending via modo-jobs

`SendEmailPayload` is a serializable mirror of `SendEmail`, designed for job queue payloads. The job handler lives in your application, not in this crate:

```rust
use modo_email::{SendEmail, SendEmailPayload, Mailer};
use modo_jobs::job;

// Enqueue from a handler:
let payload = SendEmailPayload::from(
    SendEmail::new("welcome", "user@example.com")
        .var("name", "Alice")
        .var("dashboard_url", "https://app.com/dashboard"),
);
jobs.enqueue("send_email", &payload).await?;

// Job worker:
#[job(queue = "email", max_attempts = 3, timeout = "30s")]
async fn send_email(
    Service(mailer): Service<Mailer>,
    Json(payload): Json<SendEmailPayload>,
) -> Result<(), modo::Error> {
    let email = SendEmail::from(payload);
    mailer.send(&email).await
}
```

## Custom TemplateProvider

Implement the `TemplateProvider` trait to load templates from a database, API, or any other source:

```rust
use modo_email::{EmailConfig, TemplateProvider, EmailTemplate, mailer_with};
use std::sync::Arc;

struct DbTemplateProvider {
    // your DB pool or client here
}

impl TemplateProvider for DbTemplateProvider {
    fn get(&self, name: &str, locale: &str) -> Result<EmailTemplate, modo::Error> {
        // Query your database for the template.
        let raw = todo!("load raw template string");

        // Parse the raw frontmatter + body string.
        EmailTemplate::parse(&raw)
    }
}

// Use it:
let provider = Arc::new(DbTemplateProvider { /* ... */ });
let m = mailer_with(&config, provider)?;
```

`EmailTemplate` fields if you want to build one without parsing:

```rust
EmailTemplate {
    subject: "Welcome {{name}}!".to_string(),
    body: "Hi **{{name}}**!".to_string(),
    layout: Some("default".to_string()),  // None = built-in default
}
```

## Transports and Feature Flags

| Feature  | Default | Transport       | Dependency |
| -------- | ------- | --------------- | ---------- |
| `smtp`   | Yes     | SMTP via lettre | `lettre`   |
| `resend` | No      | Resend HTTP API | `reqwest`  |

Both features can be enabled simultaneously. The active transport is selected by `transport` in config.

```toml
# SMTP only (default):
modo-email = { path = "../modo-email" }

# Resend only:
modo-email = { path = "../modo-email", default-features = false, features = ["resend"] }

# Both available:
modo-email = { path = "../modo-email", features = ["resend"] }
```

### Resend Configuration

```yaml
email:
    transport: resend
    templates_path: "emails"
    default_from_name: "My App"
    default_from_email: "hello@myapp.com"
    resend:
        api_key: "re_..."
```

## Configuration Reference

### `EmailConfig`

| Field                | Type              | Default    | Description                                 |
| -------------------- | ----------------- | ---------- | ------------------------------------------- |
| `transport`          | `smtp` / `resend` | `smtp`     | Which transport backend to use              |
| `templates_path`     | `String`          | `"emails"` | Directory containing `.md` templates        |
| `default_from_name`  | `String`          | `""`       | Default sender display name                 |
| `default_from_email` | `String`          | `""`       | Default sender email address                |
| `default_reply_to`   | `Option<String>`  | `None`     | Default reply-to address                    |
| `smtp`               | `SmtpConfig`      | see below  | SMTP settings (requires `smtp` feature)     |
| `resend`             | `ResendConfig`    | see below  | Resend settings (requires `resend` feature) |

### `SmtpConfig`

| Field      | Type     | Default       | Description          |
| ---------- | -------- | ------------- | -------------------- |
| `host`     | `String` | `"localhost"` | SMTP server hostname |
| `port`     | `u16`    | `587`         | SMTP server port     |
| `username` | `String` | `""`          | SMTP auth username   |
| `password` | `String` | `""`          | SMTP auth password   |
| `tls`      | `bool`   | `true`        | Enable STARTTLS      |

### `ResendConfig`

| Field     | Type     | Default | Description    |
| --------- | -------- | ------- | -------------- |
| `api_key` | `String` | `""`    | Resend API key |

## Key Types

| Type / Trait         | Description                                                      |
| -------------------- | ---------------------------------------------------------------- |
| `Mailer`             | Central service: render and deliver emails                       |
| `SendEmail`          | Builder for a single email send request                          |
| `SendEmailPayload`   | Serializable mirror of `SendEmail` for job queues                |
| `MailMessage`        | Fully-rendered email ready for delivery (html, text, subject...) |
| `SenderProfile`      | Sender identity (`from_name`, `from_email`, `reply_to`)          |
| `EmailTemplate`      | Parsed template (subject, body, optional layout name)            |
| `TemplateProvider`   | Trait for custom template sources                                |
| `MailTransport`      | Async trait for custom delivery backends                         |
| `FilesystemProvider` | Built-in filesystem template provider                            |
| `LayoutEngine`       | MiniJinja-based HTML layout renderer                             |
| `EmailConfig`        | Top-level config struct (transport, paths, defaults)             |
