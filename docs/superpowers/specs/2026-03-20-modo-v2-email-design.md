# modo v2 — Email Module Design Spec (Plan 6)

SMTP transport, markdown templates with YAML frontmatter, layout engine.

## Design Decisions

| Decision | Choice |
|---|---|
| Template embedding | Disk-only; optional `embed-templates` feature flag for binary embedding |
| Variable substitution | Simple `{{var}}` only — no logic, loops, or filters; safe for user-authored templates |
| Template storage | Filesystem for developer templates; app provides custom `TemplateSource` for DB or other backends |
| Sending model | Direct send only — job integration is the caller's responsibility |
| Layout engine | One built-in `base` layout (responsive, dark/light mode) + custom layouts from filesystem |
| Markdown support | Full CommonMark + `[button:type\|Label](url)` syntax |
| Plain text | Auto-generated from markdown, no override |
| SMTP connections | Connection per send (no pool) |
| Locale | Fallback chain: exact locale → default locale → no-locale path → error |
| Caching | LRU cache for `FileSource` only; custom sources manage their own caching |
| Feature flag | `email` feature, opt-in |
| Testing | `lettre::AsyncStubTransport` for integration tests — no external SMTP server needed |

## Module Structure

```
src/email/
    mod.rs          — mod declarations + pub use re-exports
    config.rs       — EmailConfig, SmtpConfig
    mailer.rs       — Mailer struct (send, render)
    message.rs      — SendEmail builder, RenderedEmail, SenderProfile
    source.rs       — TemplateSource trait + FileSource implementation
    render.rs       — variable substitution, frontmatter parsing
    markdown.rs     — pulldown-cmark → HTML + plain text, button interception
    layout.rs       — built-in base layout, custom layout loading
    cache.rs        — LRU cache (wraps FileSource)
    button.rs       — button type enum, HTML table generation
```

## Dependencies

Behind the `email` feature flag:

| Crate | Purpose |
|---|---|
| `lettre` 0.11 | SMTP transport (`tokio1`, `builder`, `smtp-transport`, `hostname`) |
| `pulldown-cmark` | CommonMark → HTML |
| `lru` | Template cache |

Dev-only (for tests):

| Crate | Purpose |
|---|---|
| `lettre` (stub-transport) | `AsyncStubTransport` for in-memory SMTP assertions |

No new dependencies for YAML frontmatter (`serde_yaml_ng` already in the project) or variable substitution (plain string replacement).

### Feature Flag

```toml
[features]
email = ["dep:lettre", "dep:pulldown-cmark", "dep:lru"]
```

## Configuration

### YAML

```yaml
email:
  templates_path: emails
  layouts_path: emails/layouts
  default_from_name: MyApp
  default_from_email: noreply@myapp.com
  default_reply_to: support@myapp.com
  default_locale: en
  cache_templates: true
  template_cache_size: 100
  smtp:
    host: smtp.mailgun.com
    port: 587
    username: postmaster@myapp.com
    password: ${SMTP_PASSWORD}
    security: starttls
```

### Rust Structs

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    pub templates_path: String,        // default: "emails"
    pub layouts_path: String,          // default: "emails/layouts"
    pub default_from_name: String,     // default: ""
    pub default_from_email: String,    // default: ""
    pub default_reply_to: Option<String>,
    pub default_locale: String,        // default: "en"
    pub cache_templates: bool,         // default: true
    pub template_cache_size: usize,    // default: 100
    pub smtp: SmtpConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SmtpConfig {
    pub host: String,                  // default: "localhost"
    pub port: u16,                     // default: 587
    pub username: Option<String>,
    pub password: Option<String>,
    pub security: SmtpSecurity,        // default: StartTls
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmtpSecurity {
    StartTls,
    Tls,
    None,
}
```

Added to `modo::Config` behind `#[cfg(feature = "email")]`, same pattern as `oauth`.

## Template Format

### Filesystem Structure

Templates live under `templates_path`, organized by locale:

```
emails/
    en/
        welcome.md
        password_reset.md
    uk/
        welcome.md
    welcome.md            ← no-locale fallback
    layouts/
        marketing.html
```

### Locale Fallback Chain

Resolution order for `render("welcome", locale: "fr")` with `default_locale: "en"`:

1. `emails/fr/welcome.md` — exact locale
2. `emails/en/welcome.md` — default locale
3. `emails/welcome.md` — no-locale fallback
4. Error if none found

### Template Syntax

```markdown
---
subject: "Welcome to {{product_name}}, {{name}}!"
layout: base
---

Hi **{{name}}**,

Thanks for signing up for **{{product_name}}**!

[button:primary|Get Started]({{dashboard_url}})

If you have questions, reply to this email.
```

### Frontmatter Fields

| Field | Required | Default | Description |
|---|---|---|---|
| `subject` | yes | — | Email subject line (supports `{{var}}`) |
| `layout` | no | `"base"` | Layout name: `"base"` for built-in, or custom from `layouts_path/` |

### Variable Substitution

Simple `{{var}}` string replacement on the entire raw template string before frontmatter parsing. Variables work in both frontmatter and body.

Rules:
- `{{var}}` — replaced with value if present, empty string if missing
- No nesting, no filters, no conditionals
- Variable names: `[a-zA-Z_][a-zA-Z0-9_]*`
- In the HTML body, variable values are HTML-escaped
- In the subject line and plain text output, values are raw (unescaped)

## Rendering Pipeline

```
Raw template string
    → [1. Substitute] {{var}} replacement on entire string
    → [2. Parse]      split YAML frontmatter from markdown body
    → [3. Render]     pulldown-cmark → HTML, intercept button syntax
    → [4. Layout]     inject HTML fragment into layout
    → [5. Plain text] second pulldown-cmark pass → plain text
```

### Stage 1: Substitute

Replace all `{{var}}` in the raw string. Body values are HTML-escaped; subject/frontmatter values are raw.

### Stage 2: Parse

Split on `---` delimiters. Deserialize YAML frontmatter into:

```rust
struct Frontmatter {
    pub subject: String,
    pub layout: Option<String>,  // defaults to "base"
}
```

Remainder is the markdown body.

### Stage 3: Render to HTML

Walk `pulldown-cmark` events. When a link text matches `button|Label` or `button:type|Label`, emit a table-based HTML button instead of `<a>`. All other CommonMark elements render normally.

### Stage 4: Layout

Load the layout by name:
- `"base"` → built-in responsive HTML layout (compiled into the crate)
- Any other name → read from `layouts_path/{name}.html`

Layout is a plain HTML file with `{{content}}` placeholder where the rendered body is injected. The layout also receives variable substitution for `brand_color`, `logo_url`, `footer_text`, etc.

### Stage 5: Plain text

Second pass over the markdown source with a plain-text emitter:
- Links → `Label (url)`
- Buttons → `Label: url`
- Bold/italic markers stripped
- Headings get a blank line above
- Lists get `- ` prefixes

### Output

```rust
pub struct RenderedEmail {
    pub subject: String,
    pub html: String,
    pub text: String,
}
```

## Button Syntax

### Format

```markdown
[button|Label](url)                  <!-- primary (default) -->
[button:primary|Label](url)
[button:danger|Delete Account](url)
[button:warning|Proceed](url)
[button:info|Learn More](url)
[button:success|Confirmed](url)
```

### ButtonType Enum

```rust
pub enum ButtonType {
    Primary,
    Danger,
    Warning,
    Info,
    Success,
}
```

### Default Colors

| Type | Background | Text |
|---|---|---|
| Primary | `#2563eb` (blue) | `#ffffff` |
| Danger | `#dc2626` (red) | `#ffffff` |
| Warning | `#d97706` (amber) | `#ffffff` |
| Info | `#0891b2` (cyan) | `#ffffff` |
| Success | `#16a34a` (green) | `#ffffff` |

If the template context includes a `brand_color` variable, `Primary` uses that instead of the default blue.

### HTML Output (Outlook-compatible)

```html
<table role="presentation" cellpadding="0" cellspacing="0" style="margin: 16px 0;">
  <tr>
    <td style="background-color: #2563eb; border-radius: 6px; padding: 12px 24px;">
      <a href="https://example.com"
         style="color: #ffffff; text-decoration: none; font-weight: 600; display: inline-block;">
        Get Started
      </a>
    </td>
  </tr>
</table>
```

### Plain Text Output

```
Get Started: https://example.com
```

## Template Source

### Trait

```rust
pub trait TemplateSource: Send + Sync {
    fn load(&self, name: &str, locale: &str, default_locale: &str) -> Result<String>;
}
```

Returns the raw template string (frontmatter + body). The `Mailer` handles rendering. `default_locale` is passed so implementations can do their own fallback logic.

### FileSource

Implements the 4-step locale fallback chain described above. Reads from `templates_path` on the filesystem.

### CachedSource Wrapper

Wraps any `TemplateSource` with LRU caching, keyed by `(name, locale)`. Only used for `FileSource` by default — custom sources are not wrapped automatically.

```rust
pub struct CachedSource<S: TemplateSource> {
    inner: S,
    cache: Mutex<LruCache<(String, String), String>>,
}
```

## Built-in Base Layout

A single responsive HTML email layout compiled into the crate.

### Features

- **Max width:** 600px centered container (email standard)
- **Responsive:** fluid on mobile via `width: 100%` with max-width
- **Padding:** 32px inner, 24px outer, 16px on small screens
- **Typography:** system font stack, 16px base, 1.5 line height
- **Dark mode:** `@media (prefers-color-scheme: dark)` — dark background (`#1a1a1a`), dark card (`#2a2a2a`), light text (`#e4e4e7`)
- **All styles inline** — email clients strip `<style>` blocks; the `<style>` tag is progressive enhancement for dark mode media query

### Layout Variables

Optional — omit them and the section doesn't render:

| Variable | Effect |
|---|---|
| `logo_url` | Renders an `<img>` above the content area |
| `footer_text` | Renders a muted text section below the card |
| `brand_color` | Overrides primary button color and accent elements |

### Custom Layouts

Plain HTML files in `layouts_path/` with `{{content}}` as the body placeholder. They receive the same variable substitution as templates.

Referenced in template frontmatter:

```yaml
---
subject: "Big news!"
layout: marketing
---
```

Resolution: `"base"` → built-in. Anything else → load from `layouts_path/{name}.html`.

## Mailer API

### Construction

```rust
// Default with FileSource (cached if configured)
let mailer = Mailer::new(&config.email)?;

// Custom source (e.g., DB-backed)
let mailer = Mailer::with_source(&config.email, my_db_source)?;

// Test with stub transport
let stub = AsyncStubTransport::new_ok();
let mailer = Mailer::with_stub_transport(&config.email, stub.clone())?;
```

### Mailer Struct

```rust
pub struct Mailer {
    source: Arc<dyn TemplateSource>,
    transport: /* enum or trait object wrapping AsyncSmtpTransport or AsyncStubTransport */,
    config: EmailConfig,
    layouts: HashMap<String, String>,  // custom layouts loaded at construction
}
```

### Methods

```rust
impl Mailer {
    /// Render template without sending
    pub fn render(&self, email: &SendEmail) -> Result<RenderedEmail>;

    /// Render and send via SMTP
    pub async fn send(&self, email: SendEmail) -> Result<()>;
}
```

`send()` calls `render()` internally, builds a `lettre::Message` with HTML + plain text multipart body, then sends via the transport.

### SendEmail Builder

```rust
pub struct SendEmail {
    pub template: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub locale: Option<String>,
    pub vars: HashMap<String, String>,
    pub sender: Option<SenderProfile>,
}

pub struct SenderProfile {
    pub from_name: String,
    pub from_email: String,
    pub reply_to: Option<String>,
}
```

```rust
let email = SendEmail::new("welcome", "user@example.com")
    .locale("uk")
    .var("name", "Dmytro")
    .var("product_name", "Modo")
    .var("dashboard_url", "https://app.example.com")
    .to("another@example.com")
    .cc("manager@example.com")
    .bcc("audit@example.com")
    .sender(SenderProfile {
        from_name: "Team".into(),
        from_email: "team@myapp.com".into(),
        reply_to: Some("team@myapp.com".into()),
    });
```

## Error Handling

All email errors use `modo::Error`:

| Scenario | Error |
|---|---|
| Template not found | `Error::not_found("email template 'welcome' not found for locale 'fr'")` |
| Frontmatter parse failure | `Error::internal("failed to parse email frontmatter: ...")` |
| Missing subject field | `Error::bad_request("email template 'welcome' missing required field 'subject'")` |
| SMTP send failure | `Error::internal("failed to send email: ...")` |
| Layout not found | `Error::not_found("email layout 'marketing' not found")` |
| Invalid button syntax | Graceful degradation — renders as a normal link |
| Empty `to` list | `Error::bad_request("email has no recipients")` |

## Testing Strategy

### Stub Transport

The `Mailer` accepts an injectable transport. In production, it uses `AsyncSmtpTransport<Tokio1Executor>`. In tests, it uses `AsyncStubTransport` from lettre:

```rust
let stub = AsyncStubTransport::new_ok();
let mailer = Mailer::with_stub_transport(&config.email, stub.clone())?;

mailer.send(SendEmail::new("welcome", "user@example.com")
    .var("name", "Dmytro")).await?;

let msgs = stub.messages().await;
assert_eq!(msgs.len(), 1);
let (envelope, raw) = &msgs[0];
assert!(envelope.to().iter().any(|a| a.as_ref() == "user@example.com"));
assert!(raw.contains("Subject: Welcome"));
```

### Unit Tests (inline `#[cfg(test)] mod tests`)

- `render.rs` — variable substitution (present, missing, special chars, HTML escaping)
- `render.rs` — frontmatter parsing (valid, missing subject, extra fields, variables in frontmatter)
- `markdown.rs` — CommonMark rendering (headings, lists, links, bold, italic, images, tables)
- `markdown.rs` — button interception (all types, default type, malformed → normal link)
- `button.rs` — HTML generation for each button type, brand_color override
- `layout.rs` — base layout injection, custom layout loading, layout variables
- `source.rs` — FileSource locale fallback chain (all 4 steps)
- `cache.rs` — LRU hit/miss/eviction
- `message.rs` — SendEmail builder (defaults, overrides, multiple recipients)

### Integration Tests (`tests/email.rs`, gated with `#![cfg(feature = "email")]`)

- End-to-end `render()`: template file → RenderedEmail with correct subject, html, text
- End-to-end `send()` with `AsyncStubTransport`: verify email captured with correct envelope and content

### Edge Cases

- Template with no frontmatter delimiter → error
- Template with empty body (frontmatter only) → valid, empty content in layout
- Variable name with invalid chars → left as literal `{{...}}`
- Nested `{{` → only outermost matched
- Button inside a list item or blockquote → rendered correctly
- Layout file with no `{{content}}` → content silently dropped
- Empty `to` list → error before SMTP attempt
- Unicode in subject/body → preserved (lettre handles MIME encoding)
