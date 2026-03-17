# Email Reference

`modo-email` provides Markdown-based email templates, responsive HTML rendering,
plain-text fallback generation, and pluggable delivery transports (SMTP via lettre
and Resend via HTTP API). The mailer is a cheap-to-clone service designed to be
registered on the jobs builder and accessed from background job handlers.

---

## Documentation

- modo-email crate: https://docs.rs/modo-email

---

## Email Service Setup

### EmailConfig

`EmailConfig` is a serde-deserializable struct loaded from YAML. All fields have
defaults so partial YAML is valid.

```rust
use modo_email::EmailConfig;

#[derive(Default, serde::Deserialize)]
pub struct Config {
    // ... other fields
    #[serde(default)]
    pub email: EmailConfig,
}
```

**Fields:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `transport` | `TransportBackend` | `"smtp"` | Which backend to use. |
| `templates_path` | `String` | `"emails"` | Directory containing `.md` template files. |
| `default_from_name` | `String` | `""` | Display name for the `From` header. |
| `default_from_email` | `String` | `""` | Email address for the `From` header. |
| `default_reply_to` | `Option<String>` | `None` | Optional default `Reply-To` address. |
| `cache_templates` | `bool` | `true` | Cache compiled templates. Set `false` in dev for live reloading. |
| `template_cache_size` | `usize` | `100` | Max cached templates (LRU eviction). Only used when `cache_templates` is `true`. |
| `smtp` | `SmtpConfig` | see below | SMTP settings (requires `smtp` feature). |
| `resend` | `ResendConfig` | see below | Resend API settings (requires `resend` feature). |

`TransportBackend` is an enum serialized as lowercase strings:

```rust
pub enum TransportBackend {
    Smtp,    // "smtp" (default)
    Resend,  // "resend"
}
```

Example YAML for SMTP:

```yaml
email:
  transport: smtp
  templates_path: emails
  default_from_name: "My App"
  default_from_email: "noreply@myapp.com"
  default_reply_to: "support@myapp.com"
  cache_templates: true
  template_cache_size: 100
  smtp:
    host: smtp.mailgun.org
    port: 587
    username: postmaster@myapp.com
    password: secret
    security: starttls
```

### SmtpConfig (requires `smtp` feature)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | `String` | `"localhost"` | SMTP server hostname. |
| `port` | `u16` | `587` | SMTP server port. |
| `username` | `String` | `""` | SMTP authentication username. |
| `password` | `String` | `""` | SMTP authentication password. |
| `security` | `SmtpSecurity` | `starttls` | TLS security mode. |

`SmtpSecurity` is an enum serialized as snake_case strings:

| Variant | YAML value | Description |
|---------|------------|-------------|
| `None` | `none` | Plaintext — no TLS (local dev or trusted networks only). |
| `StartTls` | `starttls` | Upgrade to TLS via STARTTLS command (port 587, default). |
| `ImplicitTls` | `implicit_tls` | Connect with TLS from the start — SMTPS (port 465). |

### ResendConfig (requires `resend` feature)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `api_key` | `String` | `""` | Resend API key (starts with `re_`). |

Example YAML for Resend:

```yaml
email:
  transport: resend
  templates_path: emails
  default_from_name: "My App"
  default_from_email: "noreply@myapp.com"
  resend:
    api_key: re_xxxxxxxxxxxxxxxx
```

### Cargo features

| Feature | Default | Description |
|---------|---------|-------------|
| `smtp` | yes | Enables SMTP transport via `lettre`. |
| `resend` | no | Enables Resend HTTP API transport via `reqwest`. |

Enable Resend in `Cargo.toml`:

```toml
[dependencies]
modo-email = { version = "...", default-features = false, features = ["resend"] }
```

---

## Creating a Mailer

The top-level `mailer` function is the standard entry point. It creates a `Mailer`
with a `FilesystemProvider` rooted at `config.templates_path` and the transport
selected by `config.transport`.

```rust
use modo_email::{mailer, EmailConfig};

let config = EmailConfig::default(); // or deserialized from YAML
let m = mailer(&config)?;
```

For custom template sources (database, cache, etc.), use `mailer_with`:

```rust
use modo_email::{mailer_with, EmailConfig, TemplateProvider};
use std::sync::Arc;

let provider: Arc<dyn TemplateProvider> = Arc::new(MyDbProvider::new());
let m = mailer_with(&config, provider)?;
```

### Mailer

`Mailer` ties together template loading, variable substitution, Markdown rendering,
layout wrapping, and transport delivery. It is cheaply cloneable via internal `Arc`s,
making it safe to share across async tasks and register as a service on the jobs
builder.

Key methods:

- `async Mailer::send(&self, email: &SendEmail) -> Result<(), modo::Error>` — render and
  deliver an email.
- `Mailer::render(&self, email: &SendEmail) -> Result<MailMessage, modo::Error>` —
  render to a `MailMessage` without sending (useful for testing and inspection). This is
  synchronous (not async).

---

## Markdown Templates

Templates are `.md` files with YAML frontmatter stored in the directory configured
by `templates_path` (default: `emails/`).

### File structure

```
emails/
  welcome.md          # root template (all locales)
  reset.md
  de/
    welcome.md        # German locale override
  layouts/
    default.html      # overrides the built-in default layout
    branded.html      # custom named layout
```

### Template format

Every template starts with YAML frontmatter delimited by `---`, followed by a
Markdown body:

```markdown
---
subject: "Welcome {{name}}!"
layout: default
---

Hi **{{name}}**,

Thank you for joining. Click below to get started:

[button|Launch Dashboard]({{url}})

If you have questions, [contact support](https://myapp.com/support).
```

**Frontmatter fields:**

| Field | Required | Description |
|-------|----------|-------------|
| `subject` | yes | Subject line. Supports `{{var}}` placeholders. |
| `layout` | no | Layout name to wrap the body. Defaults to `"default"`. |

Extra frontmatter fields are silently ignored.

### Variable substitution

Variables use `{{key}}` syntax. Whitespace inside braces is trimmed:
`{{ name }}` resolves the key `"name"`. Variables are substituted in both the
subject line and the Markdown body before rendering. Unresolved placeholders are
left as-is rather than producing an error.

Values substituted into the **subject line** are inserted verbatim (no HTML escaping).
Values substituted into the **Markdown body** are HTML-escaped before Markdown rendering
to prevent XSS in the rendered HTML output.

Supported value types: `&str`, `String`, `i64`, `bool`, and any type that converts
to `serde_json::Value`. Non-string JSON types use their `to_string()` representation.

### Markdown rendering

The Markdown body is rendered to both HTML and plain text in a single pass using
`pulldown-cmark`. All standard Markdown elements (headings, bold, italic, lists,
code blocks, links) are supported.

**Button links** use a special syntax and render as email-safe table-based buttons
in HTML. In plain text, they render as `Label (URL)`:

```markdown
[button|Click Here](https://example.com/action)
```

The default button color is `#4F46E5`. Override it by passing `brand_color` as a
template variable (must be a valid CSS hex color — `#RGB` or `#RRGGBB`). Invalid
values silently fall back to the default.

### Layout engine

After the Markdown body is rendered to HTML, it is wrapped in a layout using
MiniJinja. The layout receives all template variables plus two injected keys:

| Variable | Description |
|----------|-------------|
| `content` | The rendered HTML body (pre-rendered, auto-escaping disabled). |
| `subject` | The substituted subject line. |

The built-in `"default"` layout is a responsive, email-client-compatible HTML
template. It supports optional variables from the template context:

| Variable | Description |
|----------|-------------|
| `logo_url` | When set, renders a logo `<img>` above the body. |
| `product_name` | Used as the `alt` attribute on the logo image. |
| `footer_text` | Shown in the footer below the body. |

The default layout renders the `<title>` tag using `{{subject}}`.

Place custom layouts as `.html` files in `{templates_path}/layouts/`. A custom
`default.html` overrides the built-in default. Custom layouts use MiniJinja syntax:

```html
<!DOCTYPE html>
<html>
<head><title>{{ subject }}</title></head>
<body>
  <header><img src="{{ logo_url | default(value='') }}"></header>
  <main>{{ content }}</main>
  <footer>{{ footer_text | default(value='') }}</footer>
</body>
</html>
```

### Locale support

`FilesystemProvider` resolves localized templates automatically. When a locale is
set on the `SendEmail` request, the provider looks for
`{templates_path}/{locale}/{name}.md` first. If not found, it falls back to
`{templates_path}/{name}.md`. Pass an empty string (the default) to use the root
template directly.

Path traversal is rejected: template names or locales containing `..`, `/`, or `\`
return an error immediately.

---

## Sending Email

### SendEmail builder

`SendEmail` is a builder for requesting a templated email send. Construct it with
`SendEmail::new`, then chain methods before passing to `Mailer::send` or
`Mailer::render`.

```rust
use modo_email::SendEmail;

let email = SendEmail::new("welcome", "user@example.com")
    .var("name", "Alice")
    .var("url", "https://myapp.com/dashboard")
    .var("brand_color", "#ff6600")
    .var("footer_text", "© 2026 My App");
```

**Builder methods:**

| Method | Description |
|--------|-------------|
| `new(template, to)` | Create a request for `template` addressed to `to`. |
| `.to(address)` | Add an additional recipient. |
| `.locale(locale)` | Set locale for template resolution. |
| `.sender(profile)` | Override the default sender with a `SenderProfile`. |
| `.var(key, value)` | Insert a single template variable. |
| `.context(&map)` | Merge an entire `&HashMap<String, serde_json::Value>` into the context (taken by reference). |

Multiple recipients:

```rust
let email = SendEmail::new("notify", "a@example.com")
    .to("b@example.com")
    .to("c@example.com");
```

### SenderProfile

`SenderProfile` overrides the default sender configured in `EmailConfig`. It is
serializable so it can be included in job payloads.

```rust
use modo_email::SenderProfile;

let sender = SenderProfile {
    from_name: "Support Team".to_string(),
    from_email: "support@myapp.com".to_string(),
    reply_to: Some("help@myapp.com".to_string()),
};

let email = SendEmail::new("ticket", "user@example.com")
    .sender(&sender)
    .var("ticket_id", "12345");
```

`SenderProfile::format_address()` formats the address as `"Name <email>"` for the
`From` header, stripping control characters and angle brackets from the name to
prevent header injection.

### MailMessage

`MailMessage` is a fully-rendered email ready for transport. `Mailer::render` returns
this type without sending.

```rust
pub struct MailMessage {
    pub from: String,
    pub reply_to: Option<String>,
    pub to: Vec<String>,
    pub subject: String,
    pub html: String,
    pub text: String,
}
```

Use `Mailer::render` to inspect the output in tests or to preview the rendered HTML
before sending.

### SendEmailPayload

`SendEmailPayload` is a serializable mirror of `SendEmail` for use as a job queue
payload. Convert between them with the provided `From` impls:

```rust
use modo_email::{SendEmail, SendEmailPayload};

// Convert to payload before enqueuing:
let payload = SendEmailPayload::from(
    SendEmail::new("welcome", "user@example.com").var("name", "Alice")
);

// Convert back to SendEmail inside the job handler:
let email = SendEmail::from(payload);
```

`SendEmailPayload` derives `Serialize` and `Deserialize`, making it safe to store
in the job queue's JSON column.

---

## Integration Patterns

### Mailer on the jobs builder (critical)

The mailer must be registered as a service on the jobs builder with `.service()`,
not on the app builder. The app enqueues a `SendEmailPayload`; the job worker
processes the actual sending. This separation keeps HTTP handlers free of slow
network I/O.

```rust
use modo_email::{mailer, EmailConfig, SendEmail, SendEmailPayload};
use modo_jobs::JobQueue;
use modo::HandlerResult;
use modo::Service;

// In main — register mailer on the JOBS builder, not on app:
#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = modo_db::connect(&config.database).await?;
    modo_db::sync_and_migrate(&db).await?;

    let email = mailer(&config.email)?;

    let jobs = modo_jobs::new(&db, &config.jobs)
        .service(db.clone())
        .service(email)       // mailer is a jobs service, NOT an app service
        .run()
        .await?;

    app.config(config.core)
        .managed_service(db)
        .managed_service(jobs)
        .run()
        .await
}
```

### Defining the send-email job

```rust
use modo_email::{Mailer, SendEmail, SendEmailPayload};
use modo_jobs::job;
use modo::HandlerResult;
use modo::Service;

#[job(queue = "email", max_attempts = 3, timeout = "30s")]
async fn send_email(
    payload: SendEmailPayload,
    Service(mailer): Service<Mailer>,
) -> HandlerResult<()> {
    mailer.send(&SendEmail::from(payload)).await?;
    Ok(())
}
// Generates: SendEmailJob with SendEmailJob::enqueue and SendEmailJob::enqueue_at
```

### Enqueuing from an HTTP handler

```rust
use modo_jobs::JobQueue;
use modo_email::{SendEmail, SendEmailPayload};
use modo::{Json, JsonResult};
use serde_json::{Value, json};

#[modo::handler(POST, "/register")]
async fn register(queue: JobQueue, input: Json<RegisterInput>) -> JsonResult<Value> {
    // ... create user ...

    let payload = SendEmailPayload::from(
        SendEmail::new("welcome", &input.email)
            .var("name", &input.name)
    );
    SendEmailJob::enqueue(&queue, &payload).await?;

    Ok(Json(json!({ "status": "ok" })))
}
```

### Accessing Db from the email job

If the send-email job needs to look up data from the database (for example, to
load per-tenant sender profiles), register the database pool as a service on the
jobs builder and add the `Db` extractor to the job:

```rust
use modo_email::{Mailer, SendEmail, SendEmailPayload, SenderProfile};
use modo_jobs::job;
use modo::HandlerResult;
use modo::Service;
use modo_db::Db;

#[job(queue = "email", max_attempts = 3, timeout = "30s")]
async fn send_email(
    payload: SendEmailPayload,
    Service(mailer): Service<Mailer>,
    Db(db): Db,
) -> HandlerResult<()> {
    // db is Arc<DbPool> — query SeaORM entities as normal
    mailer.send(&SendEmail::from(payload)).await?;
    Ok(())
}
```

The database pool must be registered with `.service(db.clone())` on the jobs builder
for the `Db` extractor to resolve. See the jobs reference for full details.

### Custom TemplateProvider

To load templates from a database or cache instead of the filesystem, implement the
`TemplateProvider` trait and pass it to `mailer_with`:

```rust
use modo_email::{EmailTemplate, TemplateProvider};

struct DbTemplateProvider { /* db pool */ }

impl TemplateProvider for DbTemplateProvider {
    fn get(&self, name: &str, locale: &str) -> Result<EmailTemplate, modo::Error> {
        // load raw template string from database, then:
        EmailTemplate::parse(&raw)
    }
}

let provider = Arc::new(DbTemplateProvider { /* ... */ });
let m = mailer_with(&config, provider)?;
```

---

## Template Variable Syntax Overlap

Templates use `{{key}}` for runtime variable substitution. The modo-cli scaffolding
tool also uses Jinja `{{ project_name }}` syntax at scaffold time. If a template file
is generated from a scaffold and contains both scaffold-time Jinja variables and
runtime email variables, the scaffold will attempt to expand `{{name}}` as a Jinja
expression at generation time.

**Cross-reference table:**

| Context | Syntax | Expansion time |
|---------|--------|----------------|
| modo-email template body/subject | `{{name}}` | Runtime (when email is sent) |
| modo-cli scaffold template | `{{ project_name }}` | Scaffold time (when `modo new` runs) |
| Layout `.html` files | `{{ content }}` | Runtime (MiniJinja, when email is rendered) |

To include literal `{{ ... }}` in a scaffold-generated file that should survive
to runtime, wrap the runtime variable in a Jinja `{% raw %}` block:

```
{% raw %}{{name}}{% endraw %}
```

This is only relevant when a `.md` email template is generated by the CLI scaffold
and contains runtime variables. Templates authored by hand outside the scaffold
tool are unaffected.

---

## Gotchas

**Mailer on jobs builder, not app.** The most common integration error is calling
`.service(email)` on the app builder instead of the jobs builder. The app builder's
`.service()` registers services for HTTP handler extractors, not for job handlers.
The mailer must be on the jobs builder for job handlers to resolve it via
`Service<Mailer>`.

**SMTPS (port 465) requires `implicit_tls`.** Set `security: implicit_tls` and
`port: 465` for SMTPS connections. Using `security: starttls` with port 465
will fail at connection time.

**`brand_color` is validated.** Only `#RGB` and `#RRGGBB` hex strings are accepted.
Any other value (e.g. `"red"`, `"#zzzzzz"`, or a CSS injection attempt) silently
falls back to the default `#4F46E5`. Do not expect validation errors.

**Template names must not include the `.md` extension.** Pass `"welcome"` to
`SendEmail::new`, not `"welcome.md"`. The provider appends `.md` automatically.
Passing `"welcome.md"` causes the provider to look for `"welcome.md.md"`, which will
not be found.

**Layout auto-escaping is disabled.** Layout `.html` files are rendered with MiniJinja
auto-escaping turned off because `content` is already rendered HTML. If you insert
user-supplied data directly into a layout variable (other than `content`) you are
responsible for escaping it.

**`TemplateProvider::get` is called synchronously.** `FilesystemProvider` performs
blocking I/O inside `Mailer::send` (via `self.render()`, which reads template files synchronously). This is acceptable
for most workloads, but if templates are loaded from a remote store, implement
blocking access with a cache rather than making async calls inside `get`.

**Unresolved variables are left as-is.** The `vars::substitute` function leaves
`{{missing_key}}` in the output rather than replacing it with an empty string or
returning an error. A typo in a variable name produces visible `{{typo}}` text in
the rendered email.

**`SendEmailPayload` must be registered on a configured queue.** The job consuming
`SendEmailPayload` must reference a queue listed in `JobsConfig.queues`. If the
`"email"` queue is not in the configuration, `JobsBuilder::run()` returns an error
at startup.

---

## Key Types Quick Reference

| Type | docs.rs | Description |
|------|---------|-------------|
| `EmailConfig` | https://docs.rs/modo-email/latest/modo_email/struct.EmailConfig.html | Top-level email configuration. |
| `SmtpConfig` | https://docs.rs/modo-email/latest/modo_email/struct.SmtpConfig.html | SMTP connection settings (`smtp` feature). |
| `SmtpSecurity` | https://docs.rs/modo-email/latest/modo_email/enum.SmtpSecurity.html | TLS mode: `None`, `StartTls`, `ImplicitTls` (`smtp` feature). |
| `ResendConfig` | https://docs.rs/modo-email/latest/modo_email/struct.ResendConfig.html | Resend API key config (`resend` feature). |
| `TransportBackend` | https://docs.rs/modo-email/latest/modo_email/enum.TransportBackend.html | `Smtp` or `Resend` transport selector. |
| `Mailer` | https://docs.rs/modo-email/latest/modo_email/struct.Mailer.html | High-level email service (clone-safe). |
| `SendEmail` | https://docs.rs/modo-email/latest/modo_email/struct.SendEmail.html | Builder for requesting a templated send. |
| `SendEmailPayload` | https://docs.rs/modo-email/latest/modo_email/struct.SendEmailPayload.html | Serializable job queue payload. |
| `SenderProfile` | https://docs.rs/modo-email/latest/modo_email/struct.SenderProfile.html | Sender identity (name, address, reply-to). |
| `MailMessage` | https://docs.rs/modo-email/latest/modo_email/struct.MailMessage.html | Fully-rendered email ready for transport. |
| `EmailTemplate` | https://docs.rs/modo-email/latest/modo_email/template/struct.EmailTemplate.html | Parsed frontmatter + body. |
| `TemplateProvider` | https://docs.rs/modo-email/latest/modo_email/template/trait.TemplateProvider.html | Trait for custom template sources. |
| `FilesystemProvider` | https://docs.rs/modo-email/latest/modo_email/template/filesystem/struct.FilesystemProvider.html | Default filesystem-based provider. |
| `LayoutEngine` | https://docs.rs/modo-email/latest/modo_email/template/layout/struct.LayoutEngine.html | MiniJinja layout renderer. |
| `CachedTemplateProvider` | https://docs.rs/modo-email/latest/modo_email/struct.CachedTemplateProvider.html | LRU-caching wrapper around any `TemplateProvider`. Used by `mailer()` when `cache_templates = true`. |
| `MailTransport` | https://docs.rs/modo-email/latest/modo_email/transport/trait.MailTransport.html | Trait for custom delivery backends. |
| `MailTransportDyn` | https://docs.rs/modo-email/latest/modo_email/trait.MailTransportDyn.html | Object-safe, `Send`-compatible form of `MailTransport` for `Arc<dyn MailTransportDyn>`. |
| `MailTransportSend` | https://docs.rs/modo-email/latest/modo_email/trait.MailTransportSend.html | `Send`-bound alias of `MailTransport` generated by `trait-variant`. |
| `mailer` | https://docs.rs/modo-email/latest/modo_email/fn.mailer.html | Factory: filesystem provider + configured transport. |
| `mailer_with` | https://docs.rs/modo-email/latest/modo_email/fn.mailer_with.html | Factory: custom provider + configured transport. |
