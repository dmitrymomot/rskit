# modo-email Design

Email sending module for the modo framework. Pluggable transports, Markdown-based templates, multi-tenant sender customization.

## Goals

- Adapter pattern for email providers (SMTP + Resend for v1)
- Markdown templates with custom elements (buttons via `[button|Text](url)` syntax)
- Responsive HTML email output with plain text fallback
- Centralized YAML config following existing modo patterns
- Multi-tenant sender customization (same provider, different sender identity and brand context)
- Serializable payload struct for async sending via modo-jobs (job handler is app-level, not framework)
- `TemplateProvider` trait for custom template sources (DB-backed, API, etc.)
- README with documentation and usage examples

## Crate Structure

```
modo-email/
  Cargo.toml
  README.md
  src/
    lib.rs              # public API re-exports
    config.rs           # EmailConfig (YAML-deserializable)
    mailer.rs           # Mailer service
    message.rs          # SendEmail, SendEmailPayload, SenderProfile, MailMessage
    transport/
      mod.rs            # MailTransport trait + factory
      smtp.rs           # SMTP backend (lettre) — default feature
      resend.rs         # Resend HTTP API — "resend" feature
    template/
      mod.rs            # TemplateProvider trait, EmailTemplate
      filesystem.rs     # Default: loads .md files from directory
      markdown.rs       # Markdown -> HTML renderer (button support)
      layout.rs         # Wraps rendered HTML in responsive base layout
```

## Feature Flags

- `smtp` (default) — SMTP transport via `lettre`
- `resend` — Resend HTTP API via `reqwest`

## Core Traits

### MailTransport

Pluggable email delivery backend. Uses `#[async_trait]` because trait objects (`Box<dyn MailTransport>`) require object safety.

```rust
#[async_trait::async_trait]
pub trait MailTransport: Send + Sync + 'static {
    async fn send(&self, message: &MailMessage) -> Result<(), Error>;
}
```

### TemplateProvider

Pluggable template source. Sync — filesystem reads are sync, DB-backed providers can cache on init.

```rust
pub trait TemplateProvider: Send + Sync + 'static {
    fn get(&self, name: &str, locale: &str) -> Result<EmailTemplate, Error>;
}
```

Default `FilesystemProvider` resolution order:
1. `{path}/{locale}/{name}.md` — if locale provided
2. `{path}/{name}.md` — locale-less fallback (also serves single-language projects)
3. Error — template not found

## Config

YAML-deserializable, follows existing modo patterns (`UploadConfig`, `TemplateConfig`).

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    pub transport: TransportBackend,    // smtp (default) or resend
    pub templates_path: String,         // "emails"
    pub default_from_name: String,      // "My App"
    pub default_from_email: String,     // "noreply@example.com"
    pub default_reply_to: Option<String>,

    #[cfg(feature = "smtp")]
    pub smtp: SmtpConfig,

    #[cfg(feature = "resend")]
    pub resend: ResendConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SmtpConfig {
    pub host: String,       // "localhost"
    pub port: u16,          // 587
    pub username: String,
    pub password: String,
    pub tls: bool,          // true
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResendConfig {
    pub api_key: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub enum TransportBackend {
    #[default]
    Smtp,
    Resend,
}
```

YAML example:

```yaml
email:
  transport: smtp
  templates_path: emails
  default_from_name: "My App"
  default_from_email: "noreply@myapp.com"
  smtp:
    host: "smtp.gmail.com"
    port: 587
    username: "${SMTP_USER}"
    password: "${SMTP_PASS}"
    tls: true
```

## Types

### SenderProfile

Typed sender identity for multi-tenant override.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderProfile {
    pub from_name: String,
    pub from_email: String,
    pub reply_to: Option<String>,
}
```

### SendEmail

Builder for constructing an email request.

```rust
pub struct SendEmail {
    template: String,
    to: String,
    locale: Option<String>,
    sender: Option<SenderProfile>,
    context: HashMap<String, Value>,
}

impl SendEmail {
    pub fn new(template: &str, to: &str) -> Self;
    pub fn locale(mut self, locale: &str) -> Self;
    pub fn sender(mut self, sender: &SenderProfile) -> Self;
    pub fn var(mut self, key: &str, value: impl Into<Value>) -> Self;
    pub fn context(mut self, ctx: &HashMap<String, Value>) -> Self;
}
```

### SendEmailPayload

Serializable twin of `SendEmail` for job queue serialization.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendEmailPayload {
    pub template: String,
    pub to: String,
    pub locale: Option<String>,
    pub sender: Option<SenderProfile>,
    pub context: HashMap<String, Value>,
}

impl From<SendEmail> for SendEmailPayload { ... }
impl From<SendEmailPayload> for SendEmail { ... }
```

### MailMessage

Final rendered email ready for delivery.

```rust
pub struct MailMessage {
    pub from: String,           // "Acme Corp <noreply@acme.com>"
    pub reply_to: Option<String>,
    pub to: String,
    pub subject: String,
    pub html: String,           // full responsive HTML
    pub text: String,           // plain text fallback
}
```

### EmailTemplate

Raw template content before rendering.

```rust
pub struct EmailTemplate {
    pub subject: String,            // "Welcome, {{user_name}}!"
    pub body: String,               // Markdown with {{variable}} placeholders
    pub layout: Option<String>,     // layout override name, defaults to "default"
}
```

## Mailer Service

Central service tying transport, templates, and rendering together.

```rust
pub struct Mailer {
    transport: Box<dyn MailTransport>,
    templates: Box<dyn TemplateProvider>,
    default_sender: SenderProfile,
    layout_engine: LayoutEngine,
}

impl Mailer {
    /// Render and send an email.
    pub async fn send(&self, email: SendEmail) -> Result<(), Error>;

    /// Render without sending. Useful for previews and testing.
    pub fn render(&self, email: &SendEmail) -> Result<MailMessage, Error>;
}
```

### Factory Functions

```rust
/// Create a Mailer with the default FilesystemProvider.
pub fn mailer(config: &EmailConfig) -> Result<Mailer, Error>;

/// Create a Mailer with a custom TemplateProvider.
pub fn mailer_with(config: &EmailConfig, provider: Box<dyn TemplateProvider>) -> Result<Mailer, Error>;
```

## Template Rendering Pipeline

Four stages:

1. **Template loading** — `TemplateProvider.get(name, locale)` returns `EmailTemplate`
2. **Variable substitution** — simple `{{key}}` string replacement on subject and body (NOT MiniJinja, safe for end-user content). Unresolved variables left as-is.
3. **Markdown to HTML** — `pulldown-cmark` parses Markdown. Custom element detection in the event stream: link text matching `{type}|{label}` renders as a custom element. Starting with `button` type.
4. **Layout wrapping** — rendered HTML injected into a MiniJinja base layout template (developer-controlled). Layout receives `{{content}}`, `{{subject}}`, and all brand context variables. Plain text version generated from Markdown source (not from HTML).

### Template File Format

Markdown with YAML frontmatter:

```markdown
---
subject: "Welcome to {{product_name}}, {{user_name}}!"
layout: default
---

Hi **{{user_name}}**,

Thanks for signing up for **{{product_name}}**.

[button|Get Started]({{dashboard_url}})

If you have questions, reach out at {{support_email}}.
```

### Directory Structure

Single-language:

```
emails/
  layouts/
    default.html
  welcome.md
  password_reset.md
```

Multi-language (locale directories with fallback to root):

```
emails/
  layouts/
    default.html
  welcome.md              <- fallback
  de/
    welcome.md
  fr/
    welcome.md
```

### Button Rendering

`[button|Get Started](https://example.com)` is parsed by pulldown-cmark as a standard link with text `"button|Get Started"`. The renderer detects the `button|` prefix, strips it, and outputs a responsive table-based HTML button with MSO conditionals for Outlook. Button accent color comes from `brand_color` in the context, with a sensible default.

### Layout

The `default.html` layout ships as a built-in default (developer can override by placing `layouts/default.html` in their templates directory):

- Single-column, max-width 600px centered
- Inline CSS (email clients ignore `<style>` inconsistently)
- MSO conditionals for Outlook
- Dark mode via `@media (prefers-color-scheme: dark)` (progressive enhancement)
- Mobile-first: fluid width on small screens, fixed max-width on desktop

### Plain Text Generation

Derived from Markdown source (not from HTML):
- Headings: UPPERCASE
- Links: `Text (url)`
- Buttons: same as links
- Bold/italic: stripped

## Usage Examples

### Setup

```rust
#[derive(Deserialize, Default)]
struct AppConfig {
    server: ServerConfig,
    email: EmailConfig,
}

#[modo::main]
async fn main(app: AppBuilder, config: AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mailer = modo_email::mailer(&config.email)?;
    app.server_config(config.server)
        .service(mailer)
        .run()
        .await
}
```

### Simple send

```rust
#[modo::handler(POST, "/invite")]
async fn invite(mailer: Service<Mailer>, form: Form<InviteForm>) -> Result<&'static str, Error> {
    mailer.send(
        SendEmail::new("invite", &form.email)
            .var("inviter", &form.inviter_name)
    ).await?;
    Ok("Invitation sent")
}
```

### Multi-tenant with branding

```rust
mailer.send(
    SendEmail::new("welcome", "user@example.com")
        .locale("de")
        .sender(&tenant.sender_profile)
        .context(&tenant.brand_context)
        .var("user_name", "Hans")
).await?;
```

### Async via modo-jobs (app-level)

```rust
#[job(queue = "email", max_attempts = 3, timeout = "30s")]
async fn send_email(payload: SendEmailPayload, mailer: Service<Mailer>) -> Result<(), Error> {
    mailer.send(payload.into()).await
}

// Enqueue from handler
SendEmailJob::enqueue(&queue, &SendEmailPayload::from(email)).await?;
```

### Custom TemplateProvider

```rust
struct DbTemplateProvider { db: DbPool }

impl TemplateProvider for DbTemplateProvider {
    fn get(&self, name: &str, locale: &str) -> Result<EmailTemplate, Error> {
        // Load from database
    }
}

let mailer = modo_email::mailer_with(&config.email, Box::new(db_provider))?;
```

### Preview without sending

```rust
let msg = mailer.render(&email)?;
println!("{}", msg.html);
```

## Dependencies

| Crate | Purpose | Feature |
|-------|---------|---------|
| `modo` | Core framework types | always |
| `pulldown-cmark` | Markdown parsing | always |
| `minijinja` | Layout rendering | always |
| `serde` | Serialization | always |
| `serde_json` | Value type for context | always |
| `serde_yaml` | Frontmatter parsing | always |
| `async-trait` | Object-safe async traits | always |
| `lettre` | SMTP transport | `smtp` |
| `reqwest` | HTTP client for Resend | `resend` |

## Deliverables

- `modo-email/` crate with full implementation
- `modo-email/README.md` with documentation and usage examples
- Built-in responsive `default.html` layout
- Integration test suite
