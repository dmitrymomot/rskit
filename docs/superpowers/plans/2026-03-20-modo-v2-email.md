# modo v2 Email Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `email` module for modo v2 — SMTP transport with markdown templates, YAML frontmatter, variable substitution, button syntax, and a responsive base layout.

**Architecture:** The module provides a `Mailer` that loads templates from a pluggable `TemplateSource` (with a built-in `FileSource` + LRU cache), renders markdown to HTML/plain-text via `pulldown-cmark`, injects into a layout, and sends via `lettre` SMTP. Templates use `{{var}}` substitution and `[button:type|Label](url)` syntax. The module is gated behind the `email` feature flag.

**Important notes:**
- Rust 2024 edition: `std::env::set_var`/`remove_var` are `unsafe` — all tests wrap in `unsafe {}` blocks
- File organization: `mod.rs` is ONLY for `mod` imports and re-exports — all code in separate files
- `pub(crate)` items cannot be tested from integration tests (`tests/*.rs`) — use `#[cfg(test)] mod tests` inside the source file instead
- String length checks must use `.chars().count()`, not `.len()`
- Use official documentation only when researching dependencies
- Tests that modify env vars must clean up BEFORE assertions and use `serial_test` crate
- Tracing fields always snake_case
- The entire `email` module is behind the `email` feature flag — all code in `src/email/` uses `#[cfg(feature = "email")]`
- Run email tests with: `cargo test --features email-test`
- Run clippy on email code with: `cargo clippy --features email --tests -- -D warnings`

**Tech Stack:** Rust 2024 edition, lettre 0.11 (SMTP, tokio1-rustls), pulldown-cmark 0.12 (CommonMark), lru 0.12 (cache), serde_yaml_ng 0.10 (frontmatter).

**Spec:** `docs/superpowers/specs/2026-03-20-modo-v2-email-design.md`

---

## File Structure

```
Cargo.toml                          -- MODIFY: add lettre, pulldown-cmark, lru + email/email-test features
src/
  lib.rs                            -- MODIFY: add email module + re-exports
  config/
    modo.rs                         -- MODIFY: add email config field
  email/
    mod.rs                          -- mod imports + pub use re-exports
    config.rs                       -- EmailConfig, SmtpConfig, SmtpSecurity
    message.rs                      -- SendEmail builder, RenderedEmail, SenderProfile
    button.rs                       -- ButtonType enum, parse_button(), render_button_html()
    render.rs                       -- substitute(), parse_frontmatter(), Frontmatter
    markdown.rs                     -- markdown_to_html(), markdown_to_text()
    layout.rs                       -- BASE_LAYOUT const, load_layouts(), apply_layout()
    source.rs                       -- TemplateSource trait, FileSource
    cache.rs                        -- CachedSource<S>
    mailer.rs                       -- Mailer, Transport enum
tests/
  email_test.rs                     -- integration tests (gated with #![cfg(feature = "email-test")])
```

---

### Task 1: Add dependencies and feature flags to Cargo.toml

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add email dependencies**

Add to `[dependencies]` section after the auth dependencies block:

```toml
# Email (optional, gated by "email" feature)
lettre = { version = "0.11", optional = true, default-features = false, features = ["smtp-transport", "tokio1", "builder", "hostname", "tokio1-rustls"] }
pulldown-cmark = { version = "0.12", optional = true }
lru = { version = "0.16", optional = true }
```

- [ ] **Step 2: Add email and email-test feature flags**

Add to the `[features]` section:

```toml
email = ["dep:lettre", "dep:pulldown-cmark", "dep:lru"]
email-test = ["email"]
```

Update the `full` feature to include `email`:

```toml
full = ["templates", "sse", "auth", "sentry", "email"]
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add lettre, pulldown-cmark, lru for email module"
```

---

### Task 2: EmailConfig, SmtpConfig, SmtpSecurity

**Files:**
- Create: `src/email/config.rs`
- Create: `src/email/mod.rs`

- [ ] **Step 1: Write config tests**

In `src/email/config.rs`, add the structs with a test module:

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    pub templates_path: String,
    pub layouts_path: String,
    pub default_from_name: String,
    pub default_from_email: String,
    pub default_reply_to: Option<String>,
    pub default_locale: String,
    pub cache_templates: bool,
    pub template_cache_size: usize,
    pub smtp: SmtpConfig,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            templates_path: "emails".into(),
            layouts_path: "emails/layouts".into(),
            default_from_name: String::new(),
            default_from_email: String::new(),
            default_reply_to: None,
            default_locale: "en".into(),
            cache_templates: true,
            template_cache_size: 100,
            smtp: SmtpConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub security: SmtpSecurity,
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 587,
            username: None,
            password: None,
            security: SmtpSecurity::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SmtpSecurity {
    #[default]
    StartTls,
    Tls,
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_config_defaults() {
        let config = EmailConfig::default();
        assert_eq!(config.templates_path, "emails");
        assert_eq!(config.layouts_path, "emails/layouts");
        assert_eq!(config.default_from_name, "");
        assert_eq!(config.default_from_email, "");
        assert!(config.default_reply_to.is_none());
        assert_eq!(config.default_locale, "en");
        assert!(config.cache_templates);
        assert_eq!(config.template_cache_size, 100);
    }

    #[test]
    fn smtp_config_defaults() {
        let config = SmtpConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 587);
        assert!(config.username.is_none());
        assert!(config.password.is_none());
        assert_eq!(config.security, SmtpSecurity::StartTls);
    }

    #[test]
    fn email_config_from_yaml() {
        let yaml = r#"
            templates_path: custom/emails
            default_from_name: TestApp
            default_from_email: test@example.com
            default_reply_to: reply@example.com
            default_locale: uk
            cache_templates: false
            template_cache_size: 50
            smtp:
              host: smtp.example.com
              port: 465
              username: user
              password: pass
              security: tls
        "#;
        let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates_path, "custom/emails");
        assert_eq!(config.default_from_name, "TestApp");
        assert_eq!(config.default_from_email, "test@example.com");
        assert_eq!(config.default_reply_to.as_deref(), Some("reply@example.com"));
        assert_eq!(config.default_locale, "uk");
        assert!(!config.cache_templates);
        assert_eq!(config.template_cache_size, 50);
        assert_eq!(config.smtp.host, "smtp.example.com");
        assert_eq!(config.smtp.port, 465);
        assert_eq!(config.smtp.username.as_deref(), Some("user"));
        assert_eq!(config.smtp.password.as_deref(), Some("pass"));
        assert_eq!(config.smtp.security, SmtpSecurity::Tls);
    }

    #[test]
    fn email_config_partial_yaml_uses_defaults() {
        let yaml = r#"
            default_from_email: noreply@app.com
        "#;
        let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates_path, "emails");
        assert_eq!(config.default_from_email, "noreply@app.com");
        assert_eq!(config.smtp.host, "localhost");
        assert_eq!(config.smtp.port, 587);
    }

    #[test]
    fn smtp_security_none_variant() {
        let yaml = r#"security: none"#;
        let config: SmtpConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.security, SmtpSecurity::None);
    }
}
```

- [ ] **Step 2: Create mod.rs**

Create `src/email/mod.rs`:

```rust
mod config;

pub use config::{EmailConfig, SmtpConfig, SmtpSecurity};
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features email --lib -- email::config::tests -v`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/email/
git commit -m "feat(email): add EmailConfig, SmtpConfig, SmtpSecurity with defaults"
```

---

### Task 3: Wire email config into modo::Config and lib.rs

**Files:**
- Modify: `src/config/modo.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add email config field to modo::Config**

In `src/config/modo.rs`, add after the `oauth` field:

```rust
#[cfg(feature = "email")]
#[serde(default)]
pub email: crate::email::EmailConfig,
```

- [ ] **Step 2: Add email module to lib.rs**

In `src/lib.rs`, add after the `auth` module:

```rust
#[cfg(feature = "email")]
pub mod email;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check --features email`
Expected: compiles with no errors.

- [ ] **Step 4: Commit**

```bash
git add src/config/modo.rs src/lib.rs
git commit -m "feat(email): wire email module and config into modo"
```

---

### Task 4: SendEmail builder, RenderedEmail, SenderProfile

**Files:**
- Create: `src/email/message.rs`
- Modify: `src/email/mod.rs`

- [ ] **Step 1: Write message types with tests**

Create `src/email/message.rs`:

```rust
use std::collections::HashMap;

/// A rendered email ready for sending.
pub struct RenderedEmail {
    pub subject: String,
    pub html: String,
    pub text: String,
}

/// Overrides the default sender for a specific email.
pub struct SenderProfile {
    pub from_name: String,
    pub from_email: String,
    pub reply_to: Option<String>,
}

/// Builder for composing an email to send.
pub struct SendEmail {
    pub template: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub locale: Option<String>,
    pub vars: HashMap<String, String>,
    pub sender: Option<SenderProfile>,
}

impl SendEmail {
    pub fn new(template: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            template: template.into(),
            to: vec![to.into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            locale: None,
            vars: HashMap::new(),
            sender: None,
        }
    }

    pub fn to(mut self, addr: impl Into<String>) -> Self {
        self.to.push(addr.into());
        self
    }

    pub fn cc(mut self, addr: impl Into<String>) -> Self {
        self.cc.push(addr.into());
        self
    }

    pub fn bcc(mut self, addr: impl Into<String>) -> Self {
        self.bcc.push(addr.into());
        self
    }

    pub fn locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = Some(locale.into());
        self
    }

    pub fn var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.vars.insert(key.into(), value.into());
        self
    }

    pub fn sender(mut self, profile: SenderProfile) -> Self {
        self.sender = Some(profile);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_template_and_first_recipient() {
        let email = SendEmail::new("welcome", "user@example.com");
        assert_eq!(email.template, "welcome");
        assert_eq!(email.to, vec!["user@example.com"]);
        assert!(email.cc.is_empty());
        assert!(email.bcc.is_empty());
        assert!(email.locale.is_none());
        assert!(email.vars.is_empty());
        assert!(email.sender.is_none());
    }

    #[test]
    fn builder_chain() {
        let email = SendEmail::new("reset", "a@example.com")
            .to("b@example.com")
            .cc("c@example.com")
            .bcc("d@example.com")
            .locale("uk")
            .var("name", "Dmytro")
            .var("token", "abc123")
            .sender(SenderProfile {
                from_name: "Support".into(),
                from_email: "support@app.com".into(),
                reply_to: Some("help@app.com".into()),
            });
        assert_eq!(email.to, vec!["a@example.com", "b@example.com"]);
        assert_eq!(email.cc, vec!["c@example.com"]);
        assert_eq!(email.bcc, vec!["d@example.com"]);
        assert_eq!(email.locale.as_deref(), Some("uk"));
        assert_eq!(email.vars.get("name").unwrap(), "Dmytro");
        assert_eq!(email.vars.get("token").unwrap(), "abc123");
        let sender = email.sender.unwrap();
        assert_eq!(sender.from_name, "Support");
        assert_eq!(sender.from_email, "support@app.com");
        assert_eq!(sender.reply_to.as_deref(), Some("help@app.com"));
    }

    #[test]
    fn var_overwrites_previous_value() {
        let email = SendEmail::new("t", "a@b.com")
            .var("key", "old")
            .var("key", "new");
        assert_eq!(email.vars.get("key").unwrap(), "new");
    }
}
```

- [ ] **Step 2: Add to mod.rs**

Add to `src/email/mod.rs`:

```rust
mod message;

pub use message::{RenderedEmail, SendEmail, SenderProfile};
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features email --lib -- email::message::tests -v`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/email/message.rs src/email/mod.rs
git commit -m "feat(email): add SendEmail builder, RenderedEmail, SenderProfile"
```

---

### Task 5: ButtonType enum and HTML generation

**Files:**
- Create: `src/email/button.rs`
- Modify: `src/email/mod.rs`

- [ ] **Step 1: Write button module with tests**

Create `src/email/button.rs`:

```rust
/// Button type variants for email buttons.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonType {
    Primary,
    Danger,
    Warning,
    Info,
    Success,
}

impl ButtonType {
    /// Returns (background_color, text_color) for this button type.
    pub fn colors(&self, brand_color: Option<&str>) -> (&str, &str) {
        match self {
            Self::Primary => (brand_color.unwrap_or("#2563eb"), "#ffffff"),
            Self::Danger => ("#dc2626", "#ffffff"),
            Self::Warning => ("#d97706", "#ffffff"),
            Self::Info => ("#0891b2", "#ffffff"),
            Self::Success => ("#16a34a", "#ffffff"),
        }
    }
}

/// Parse button text like "button|Label" or "button:type|Label".
/// Returns `Some((ButtonType, label))` if it matches, `None` otherwise.
pub fn parse_button(text: &str) -> Option<(ButtonType, &str)> {
    let rest = text.strip_prefix("button")?;

    if let Some(rest) = rest.strip_prefix('|') {
        // "button|Label" -> Primary
        if rest.is_empty() {
            return None;
        }
        return Some((ButtonType::Primary, rest));
    }

    if let Some(rest) = rest.strip_prefix(':') {
        // "button:type|Label"
        let (type_str, label) = rest.split_once('|')?;
        if label.is_empty() {
            return None;
        }
        let btn_type = match type_str {
            "primary" => ButtonType::Primary,
            "danger" => ButtonType::Danger,
            "warning" => ButtonType::Warning,
            "info" => ButtonType::Info,
            "success" => ButtonType::Success,
            _ => return None,
        };
        return Some((btn_type, label));
    }

    None
}

/// Render a table-based HTML button (Outlook-compatible).
pub fn render_button_html(label: &str, url: &str, btn_type: ButtonType, brand_color: Option<&str>) -> String {
    let (bg, fg) = btn_type.colors(brand_color);
    format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin: 16px 0;"><tr><td style="background-color: {bg}; border-radius: 6px; padding: 12px 24px;"><a href="{url}" style="color: {fg}; text-decoration: none; font-weight: 600; display: inline-block;">{label}</a></td></tr></table>"#
    )
}

/// Render a plain text button.
pub fn render_button_text(label: &str, url: &str) -> String {
    format!("{label}: {url}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_button_primary_default() {
        let (btn_type, label) = parse_button("button|Get Started").unwrap();
        assert_eq!(btn_type, ButtonType::Primary);
        assert_eq!(label, "Get Started");
    }

    #[test]
    fn parse_button_with_type() {
        let (btn_type, label) = parse_button("button:danger|Delete Account").unwrap();
        assert_eq!(btn_type, ButtonType::Danger);
        assert_eq!(label, "Delete Account");
    }

    #[test]
    fn parse_button_all_types() {
        assert_eq!(parse_button("button:primary|X").unwrap().0, ButtonType::Primary);
        assert_eq!(parse_button("button:danger|X").unwrap().0, ButtonType::Danger);
        assert_eq!(parse_button("button:warning|X").unwrap().0, ButtonType::Warning);
        assert_eq!(parse_button("button:info|X").unwrap().0, ButtonType::Info);
        assert_eq!(parse_button("button:success|X").unwrap().0, ButtonType::Success);
    }

    #[test]
    fn parse_button_not_a_button() {
        assert!(parse_button("Click here").is_none());
        assert!(parse_button("").is_none());
        assert!(parse_button("button").is_none());
        assert!(parse_button("button|").is_none());
        assert!(parse_button("button:unknown|Label").is_none());
        assert!(parse_button("button:danger|").is_none());
    }

    #[test]
    fn render_html_contains_expected_parts() {
        let html = render_button_html("Go", "https://x.com", ButtonType::Primary, None);
        assert!(html.contains("background-color: #2563eb"));
        assert!(html.contains("href=\"https://x.com\""));
        assert!(html.contains(">Go</a>"));
        assert!(html.contains("role=\"presentation\""));
    }

    #[test]
    fn render_html_brand_color_overrides_primary() {
        let html = render_button_html("Go", "https://x.com", ButtonType::Primary, Some("#ff0000"));
        assert!(html.contains("background-color: #ff0000"));
    }

    #[test]
    fn render_html_brand_color_does_not_affect_other_types() {
        let html = render_button_html("Go", "https://x.com", ButtonType::Danger, Some("#ff0000"));
        assert!(html.contains("background-color: #dc2626"));
    }

    #[test]
    fn render_text_format() {
        let text = render_button_text("Get Started", "https://example.com");
        assert_eq!(text, "Get Started: https://example.com");
    }
}
```

- [ ] **Step 2: Add to mod.rs**

Add to `src/email/mod.rs`:

```rust
mod button;

pub use button::ButtonType;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features email --lib -- email::button::tests -v`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/email/button.rs src/email/mod.rs
git commit -m "feat(email): add ButtonType enum, parse and render functions"
```

---

### Task 6: Variable substitution and frontmatter parsing

**Files:**
- Create: `src/email/render.rs`
- Modify: `src/email/mod.rs`

- [ ] **Step 1: Write render module with tests**

Create `src/email/render.rs`:

```rust
use crate::{Error, Result};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
pub struct Frontmatter {
    pub subject: String,
    #[serde(default = "default_layout")]
    pub layout: String,
}

fn default_layout() -> String {
    "base".into()
}

static VAR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}").unwrap());

/// Replace all `{{var}}` in the input string with raw values from the vars map.
/// Missing variables are replaced with empty strings.
pub fn substitute(input: &str, vars: &HashMap<String, String>) -> String {
    VAR_RE
        .replace_all(input, |caps: &regex::Captures| {
            vars.get(&caps[1]).cloned().unwrap_or_default()
        })
        .into_owned()
}

/// Split a template string into frontmatter and body.
/// Template must start with `---\n` and have a closing `---\n`.
pub fn parse_frontmatter(template: &str) -> Result<(Frontmatter, &str)> {
    let template = template.trim_start();

    if !template.starts_with("---") {
        return Err(Error::bad_request(
            "email template missing frontmatter delimiter '---'",
        ));
    }

    let after_first = &template[3..];
    let after_first = after_first.strip_prefix('\n').unwrap_or(after_first);

    let end = after_first.find("\n---").ok_or_else(|| {
        Error::bad_request("email template missing closing frontmatter delimiter '---'")
    })?;

    let yaml = &after_first[..end];
    let body = &after_first[end + 4..]; // skip "\n---"
    let body = body.strip_prefix('\n').unwrap_or(body);

    let frontmatter: Frontmatter = serde_yaml_ng::from_str(yaml)
        .map_err(|e| Error::internal(format!("failed to parse email frontmatter: {e}")))?;

    if frontmatter.subject.is_empty() {
        return Err(Error::bad_request(
            "email template missing required field 'subject'",
        ));
    }

    Ok((frontmatter, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_replaces_known_vars() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), "Dmytro".into());
        vars.insert("product".into(), "Modo".into());
        let result = substitute("Hello {{name}}, welcome to {{product}}!", &vars);
        assert_eq!(result, "Hello Dmytro, welcome to Modo!");
    }

    #[test]
    fn substitute_missing_var_becomes_empty() {
        let vars = HashMap::new();
        let result = substitute("Hello {{name}}!", &vars);
        assert_eq!(result, "Hello !");
    }

    #[test]
    fn substitute_preserves_invalid_var_names() {
        let vars = HashMap::new();
        let result = substitute("Hello {{123invalid}}!", &vars);
        assert_eq!(result, "Hello {{123invalid}}!");
    }

    #[test]
    fn substitute_no_vars_in_template() {
        let vars = HashMap::new();
        let result = substitute("Hello world!", &vars);
        assert_eq!(result, "Hello world!");
    }

    #[test]
    fn substitute_special_chars_in_value() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), "<b>Bold</b>".into());
        let result = substitute("Hello {{name}}!", &vars);
        assert_eq!(result, "Hello <b>Bold</b>!");
    }

    #[test]
    fn substitute_vars_in_frontmatter() {
        let mut vars = HashMap::new();
        vars.insert("product".into(), "Modo".into());
        vars.insert("name".into(), "Dmytro".into());
        let template = "---\nsubject: \"Welcome to {{product}}, {{name}}!\"\n---\nBody";
        let result = substitute(template, &vars);
        assert!(result.contains("Welcome to Modo, Dmytro!"));
    }

    #[test]
    fn parse_frontmatter_valid() {
        let template = "---\nsubject: Welcome!\nlayout: custom\n---\nHello body";
        let (fm, body) = parse_frontmatter(template).unwrap();
        assert_eq!(fm.subject, "Welcome!");
        assert_eq!(fm.layout, "custom");
        assert_eq!(body, "Hello body");
    }

    #[test]
    fn parse_frontmatter_default_layout() {
        let template = "---\nsubject: Hello\n---\nBody";
        let (fm, _) = parse_frontmatter(template).unwrap();
        assert_eq!(fm.layout, "base");
    }

    #[test]
    fn parse_frontmatter_empty_body() {
        let template = "---\nsubject: Hello\n---\n";
        let (fm, body) = parse_frontmatter(template).unwrap();
        assert_eq!(fm.subject, "Hello");
        assert!(body.is_empty());
    }

    #[test]
    fn parse_frontmatter_missing_delimiter() {
        let result = parse_frontmatter("No frontmatter here");
        assert!(result.is_err());
    }

    #[test]
    fn parse_frontmatter_missing_closing_delimiter() {
        let result = parse_frontmatter("---\nsubject: Hello\nNo closing");
        assert!(result.is_err());
    }

    #[test]
    fn parse_frontmatter_missing_subject() {
        let result = parse_frontmatter("---\nlayout: base\n---\nBody");
        assert!(result.is_err());
    }

    #[test]
    fn parse_frontmatter_empty_subject() {
        let result = parse_frontmatter("---\nsubject: \"\"\n---\nBody");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Add to mod.rs**

Add to `src/email/mod.rs`:

```rust
mod render;
```

(`render` internals are used by `mailer.rs` — no public re-exports needed.)

- [ ] **Step 3: Run tests**

Run: `cargo test --features email --lib -- email::render::tests -v`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/email/render.rs src/email/mod.rs
git commit -m "feat(email): add variable substitution and frontmatter parsing"
```

---

### Task 7: Markdown to HTML rendering with button interception

**Files:**
- Create: `src/email/markdown.rs`
- Modify: `src/email/mod.rs`

- [ ] **Step 1: Write markdown module with tests**

Create `src/email/markdown.rs`. This module walks `pulldown-cmark` events. When it encounters a link whose text matches the button syntax, it emits table-based HTML instead of `<a>`. For plain text, it does a second pass stripping markdown to text.

```rust
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd};

use crate::email::button;

/// HTML-escape a string for use in attribute values.
fn escape_href(url: &str) -> String {
    url.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Convert markdown to HTML, intercepting button syntax in links.
///
/// Strategy: buffer all events between `Start(Link)` and `End(Link)`,
/// then check if the concatenated text matches button syntax.
/// If yes, emit a table-based button. If no, flush all buffered events
/// as a normal link through `push_html`.
pub fn markdown_to_html(markdown: &str, brand_color: Option<&str>) -> String {
    let parser = Parser::new_ext(markdown, Options::all());
    let mut html = String::new();

    // Link buffering state
    let mut link_url: Option<String> = None;
    let mut link_title: Option<CowStr> = None;
    let mut link_events: Vec<Event> = Vec::new();

    for event in parser {
        if link_url.is_some() {
            match &event {
                Event::End(TagEnd::Link) => {
                    let url = link_url.take().unwrap();
                    let title = link_title.take();

                    // Concatenate all text events to check for button syntax
                    let full_text: String = link_events
                        .iter()
                        .filter_map(|e| match e {
                            Event::Text(t) => Some(t.as_ref()),
                            Event::Code(t) => Some(t.as_ref()),
                            _ => None,
                        })
                        .collect();

                    if let Some((btn_type, label)) = button::parse_button(&full_text) {
                        // Emit table-based button
                        html.push_str(&button::render_button_html(
                            label,
                            &url,
                            btn_type,
                            brand_color,
                        ));
                    } else {
                        // Flush as normal link: re-wrap in Start(Link) + events + End(Link)
                        let start = Event::Start(Tag::Link {
                            link_type: pulldown_cmark::LinkType::Inline,
                            dest_url: CowStr::from(url),
                            title: title.unwrap_or(CowStr::from("")),
                            id: CowStr::from(""),
                        });
                        let end = Event::End(TagEnd::Link);
                        let full_events: Vec<Event> = std::iter::once(start)
                            .chain(link_events.drain(..))
                            .chain(std::iter::once(end))
                            .collect();
                        pulldown_cmark::html::push_html(&mut html, full_events.into_iter());
                    }

                    link_events.clear();
                }
                _ => {
                    link_events.push(event);
                }
            }
        } else {
            match event {
                Event::Start(Tag::Link {
                    dest_url, title, ..
                }) => {
                    link_url = Some(dest_url.to_string());
                    link_title = Some(title);
                    link_events.clear();
                }
                _ => {
                    pulldown_cmark::html::push_html(&mut html, std::iter::once(event));
                }
            }
        }
    }

    html
}

/// Convert markdown to plain text.
pub fn markdown_to_text(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, Options::all());
    let mut text = String::new();
    let mut in_link: Option<String> = None; // holds URL
    let mut link_text = String::new();
    let mut in_heading = false;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                in_heading = true;
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push('\n');
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                text.push('\n');
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                in_link = Some(dest_url.to_string());
                link_text.clear();
            }
            Event::Text(t) if in_link.is_some() => {
                link_text.push_str(&t);
            }
            Event::End(TagEnd::Link) => {
                if let Some(url) = in_link.take() {
                    if let Some((_, label)) = button::parse_button(&link_text) {
                        text.push_str(&button::render_button_text(label, &url));
                    } else {
                        text.push_str(&format!("{link_text} ({url})"));
                    }
                    link_text.clear();
                }
            }
            Event::Start(Tag::Item) => {
                text.push_str("- ");
            }
            Event::End(TagEnd::Item) => {
                if !text.ends_with('\n') {
                    text.push('\n');
                }
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                text.push_str("\n\n");
            }
            Event::Text(t) => {
                text.push_str(&t);
            }
            Event::SoftBreak | Event::HardBreak => {
                text.push('\n');
            }
            Event::Code(t) => {
                text.push_str(&t);
            }
            _ => {}
        }
    }

    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_basic_paragraph() {
        let html = markdown_to_html("Hello **world**!", None);
        assert!(html.contains("<strong>world</strong>"));
    }

    #[test]
    fn html_heading() {
        let html = markdown_to_html("# Title\n\nBody", None);
        assert!(html.contains("<h1>Title</h1>"));
    }

    #[test]
    fn html_link() {
        let html = markdown_to_html("[Click](https://example.com)", None);
        assert!(html.contains("<a href=\"https://example.com\">Click</a>"));
    }

    #[test]
    fn html_button_primary_default() {
        let html = markdown_to_html("[button|Get Started](https://example.com)", None);
        assert!(html.contains("role=\"presentation\""));
        assert!(html.contains("background-color: #2563eb"));
        assert!(html.contains(">Get Started</a>"));
        assert!(html.contains("href=\"https://example.com\""));
    }

    #[test]
    fn html_button_with_type() {
        let html = markdown_to_html("[button:danger|Delete](https://example.com)", None);
        assert!(html.contains("background-color: #dc2626"));
        assert!(html.contains(">Delete</a>"));
    }

    #[test]
    fn html_button_brand_color() {
        let html = markdown_to_html(
            "[button|Click](https://example.com)",
            Some("#ff0000"),
        );
        assert!(html.contains("background-color: #ff0000"));
    }

    #[test]
    fn html_malformed_button_renders_as_link() {
        let html = markdown_to_html("[button:unknown|Click](https://example.com)", None);
        assert!(html.contains("<a href="));
        assert!(!html.contains("role=\"presentation\""));
    }

    #[test]
    fn html_list() {
        let html = markdown_to_html("- Item 1\n- Item 2", None);
        assert!(html.contains("<li>"));
    }

    #[test]
    fn text_basic_paragraph() {
        let text = markdown_to_text("Hello **world**!");
        assert_eq!(text, "Hello world!");
    }

    #[test]
    fn text_link() {
        let text = markdown_to_text("[Click](https://example.com)");
        assert_eq!(text, "Click (https://example.com)");
    }

    #[test]
    fn text_button() {
        let text = markdown_to_text("[button:primary|Get Started](https://example.com)");
        assert_eq!(text, "Get Started: https://example.com");
    }

    #[test]
    fn text_heading() {
        let text = markdown_to_text("# Title\n\nBody");
        assert!(text.contains("Title"));
        assert!(text.contains("Body"));
    }

    #[test]
    fn text_list() {
        let text = markdown_to_text("- Item 1\n- Item 2");
        assert!(text.contains("- Item 1"));
        assert!(text.contains("- Item 2"));
    }
}
```

- [ ] **Step 2: Add to mod.rs**

Add to `src/email/mod.rs`:

```rust
mod markdown;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features email --lib -- email::markdown::tests -v`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/email/markdown.rs src/email/mod.rs
git commit -m "feat(email): add markdown-to-HTML/text rendering with button interception"
```

---

### Task 8: Built-in base layout and layout loading

**Files:**
- Create: `src/email/layout.rs`
- Modify: `src/email/mod.rs`

- [ ] **Step 1: Write layout module with tests**

Create `src/email/layout.rs`:

```rust
use crate::{Error, Result};
use std::collections::HashMap;
use std::path::Path;

use crate::email::render;

/// Built-in responsive base email layout.
/// Features: 600px max-width, dark mode, system font, inline styles.
pub const BASE_LAYOUT: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="color-scheme" content="light dark">
<meta name="supported-color-schemes" content="light dark">
<style>
@media (prefers-color-scheme: dark) {
  body { background-color: #1a1a1a !important; }
  .email-card { background-color: #2a2a2a !important; }
  .email-card * { color: #e4e4e7 !important; }
  .email-footer { color: #a1a1aa !important; }
}
@media only screen and (max-width: 620px) {
  .email-outer { padding: 16px 8px !important; }
  .email-card { padding: 24px 16px !important; }
}
</style>
</head>
<body style="margin: 0; padding: 0; background-color: #f4f4f5; -webkit-font-smoothing: antialiased;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background-color: #f4f4f5;">
<tr>
<td class="email-outer" align="center" style="padding: 24px 16px;">
<!--[if mso]><table role="presentation" width="600" cellpadding="0" cellspacing="0"><tr><td><![endif]-->
<table role="presentation" cellpadding="0" cellspacing="0" style="max-width: 600px; width: 100%;">
{{logo_section}}
<tr>
<td class="email-card" style="background-color: #ffffff; padding: 32px; border-radius: 8px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; font-size: 16px; line-height: 1.5; color: #18181b;">
{{content}}
</td>
</tr>
{{footer_section}}
</table>
<!--[if mso]></td></tr></table><![endif]-->
</td>
</tr>
</table>
</body>
</html>"##;

const LOGO_SECTION: &str = r#"<tr><td align="center" style="padding-bottom: 24px;"><img src="{{logo_url}}" alt="Logo" style="max-width: 150px; height: auto;" /></td></tr>"#;

const FOOTER_SECTION: &str = r#"<tr><td class="email-footer" align="center" style="padding-top: 24px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; font-size: 13px; color: #71717a;">{{footer_text}}</td></tr>"#;

/// Load custom layouts from the given directory.
/// Returns a map of layout name -> layout HTML content.
pub fn load_layouts(layouts_path: &str) -> Result<HashMap<String, String>> {
    let path = Path::new(layouts_path);
    let mut layouts = HashMap::new();

    if !path.exists() {
        return Ok(layouts);
    }

    let entries = std::fs::read_dir(path)
        .map_err(|e| Error::internal(format!("failed to read layouts directory: {e}")))?;

    for entry in entries {
        let entry =
            entry.map_err(|e| Error::internal(format!("failed to read layout entry: {e}")))?;
        let file_path = entry.path();

        if file_path.extension().and_then(|e| e.to_str()) == Some("html") {
            if let Some(name) = file_path.file_stem().and_then(|s| s.to_str()) {
                let content = std::fs::read_to_string(&file_path).map_err(|e| {
                    Error::internal(format!("failed to read layout '{}': {e}", file_path.display()))
                })?;
                layouts.insert(name.to_string(), content);
            }
        }
    }

    Ok(layouts)
}

/// Apply a layout to rendered HTML content.
/// Resolves `{{content}}`, `{{logo_section}}`, `{{footer_section}}`, and all vars.
pub fn apply_layout(
    layout_html: &str,
    content: &str,
    vars: &HashMap<String, String>,
) -> String {
    // Resolve conditional sections in the base layout
    let logo_section = if vars.contains_key("logo_url") {
        render::substitute(LOGO_SECTION, vars)
    } else {
        String::new()
    };

    let footer_section = if vars.contains_key("footer_text") {
        render::substitute(FOOTER_SECTION, vars)
    } else {
        String::new()
    };

    // Build a vars map that includes the special placeholders
    let mut full_vars = vars.clone();
    full_vars.insert("content".into(), content.into());
    full_vars.insert("logo_section".into(), logo_section);
    full_vars.insert("footer_section".into(), footer_section);

    render::substitute(layout_html, &full_vars)
}

/// Resolve a layout name to its HTML content.
/// "base" returns the built-in layout; anything else looks up the custom layouts map.
pub fn resolve_layout<'a>(
    name: &str,
    custom_layouts: &'a HashMap<String, String>,
) -> Result<std::borrow::Cow<'a, str>> {
    if name == "base" {
        Ok(std::borrow::Cow::Borrowed(BASE_LAYOUT))
    } else {
        custom_layouts
            .get(name)
            .map(|s| std::borrow::Cow::Borrowed(s.as_str()))
            .ok_or_else(|| Error::not_found(format!("email layout '{name}' not found")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_layout_has_content_placeholder() {
        assert!(BASE_LAYOUT.contains("{{content}}"));
    }

    #[test]
    fn base_layout_has_dark_mode() {
        assert!(BASE_LAYOUT.contains("prefers-color-scheme: dark"));
    }

    #[test]
    fn base_layout_has_max_width() {
        assert!(BASE_LAYOUT.contains("max-width: 600px"));
    }

    #[test]
    fn apply_layout_injects_content() {
        let layout = "<div>{{content}}</div>";
        let result = apply_layout(layout, "<p>Hello</p>", &HashMap::new());
        assert_eq!(result, "<div><p>Hello</p></div>");
    }

    #[test]
    fn apply_layout_substitutes_vars() {
        let layout = "<div style=\"color: {{brand_color}}\">{{content}}</div>";
        let mut vars = HashMap::new();
        vars.insert("brand_color".into(), "#ff0000".into());
        let result = apply_layout(layout, "Body", &vars);
        assert!(result.contains("color: #ff0000"));
    }

    #[test]
    fn apply_layout_logo_section_when_var_present() {
        let mut vars = HashMap::new();
        vars.insert("logo_url".into(), "https://example.com/logo.png".into());
        let result = apply_layout(BASE_LAYOUT, "<p>Hello</p>", &vars);
        assert!(result.contains("https://example.com/logo.png"));
        assert!(result.contains("<img"));
    }

    #[test]
    fn apply_layout_no_logo_when_var_absent() {
        let result = apply_layout(BASE_LAYOUT, "<p>Hello</p>", &HashMap::new());
        assert!(!result.contains("<img"));
    }

    #[test]
    fn apply_layout_footer_section_when_var_present() {
        let mut vars = HashMap::new();
        vars.insert("footer_text".into(), "Copyright 2026".into());
        let result = apply_layout(BASE_LAYOUT, "<p>Hello</p>", &vars);
        assert!(result.contains("Copyright 2026"));
    }

    #[test]
    fn apply_layout_no_footer_when_var_absent() {
        let result = apply_layout(BASE_LAYOUT, "<p>Hello</p>", &HashMap::new());
        // The CSS rule .email-footer is always in <style>, but the actual
        // <td class="email-footer"> element should not be rendered
        assert!(!result.contains(r#"class="email-footer""#));
    }

    #[test]
    fn resolve_layout_base() {
        let customs = HashMap::new();
        let layout = resolve_layout("base", &customs).unwrap();
        assert!(layout.contains("{{content}}"));
    }

    #[test]
    fn resolve_layout_custom_found() {
        let mut customs = HashMap::new();
        customs.insert("marketing".into(), "<html>{{content}}</html>".into());
        let layout = resolve_layout("marketing", &customs).unwrap();
        assert_eq!(layout.as_ref(), "<html>{{content}}</html>");
    }

    #[test]
    fn resolve_layout_custom_not_found() {
        let customs = HashMap::new();
        let result = resolve_layout("missing", &customs);
        assert!(result.is_err());
    }

    #[test]
    fn load_layouts_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let layouts = load_layouts(dir.path().to_str().unwrap()).unwrap();
        assert!(layouts.is_empty());
    }

    #[test]
    fn load_layouts_reads_html_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("custom.html"), "<div>{{content}}</div>").unwrap();
        std::fs::write(dir.path().join("ignore.txt"), "not a layout").unwrap();
        let layouts = load_layouts(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(layouts.len(), 1);
        assert!(layouts.contains_key("custom"));
        assert_eq!(layouts["custom"], "<div>{{content}}</div>");
    }

    #[test]
    fn load_layouts_nonexistent_dir_returns_empty() {
        let layouts = load_layouts("/nonexistent/path/that/does/not/exist").unwrap();
        assert!(layouts.is_empty());
    }
}
```

- [ ] **Step 2: Add to mod.rs**

Add to `src/email/mod.rs`:

```rust
mod layout;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features email --lib -- email::layout::tests -v`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/email/layout.rs src/email/mod.rs
git commit -m "feat(email): add built-in base layout and custom layout loading"
```

---

### Task 9: TemplateSource trait and FileSource

**Files:**
- Create: `src/email/source.rs`
- Modify: `src/email/mod.rs`

- [ ] **Step 1: Write source module with tests**

Create `src/email/source.rs`:

```rust
use crate::{Error, Result};
use std::path::{Path, PathBuf};

/// Trait for loading raw email templates (frontmatter + body).
/// Implementations must be `Send + Sync` for use in `Arc<dyn TemplateSource>`.
pub trait TemplateSource: Send + Sync {
    fn load(&self, name: &str, locale: &str, default_locale: &str) -> Result<String>;
}

/// Loads templates from the filesystem with locale fallback.
///
/// Fallback chain:
/// 1. `{path}/{locale}/{name}.md`
/// 2. `{path}/{default_locale}/{name}.md`
/// 3. `{path}/{name}.md`
/// 4. Error
pub struct FileSource {
    path: PathBuf,
}

impl FileSource {
    pub fn new(templates_path: impl Into<PathBuf>) -> Self {
        Self {
            path: templates_path.into(),
        }
    }

    fn try_load(&self, file_path: &Path) -> Option<String> {
        std::fs::read_to_string(file_path).ok()
    }
}

impl TemplateSource for FileSource {
    fn load(&self, name: &str, locale: &str, default_locale: &str) -> Result<String> {
        let filename = format!("{name}.md");

        // 1. Exact locale
        let path = self.path.join(locale).join(&filename);
        if let Some(content) = self.try_load(&path) {
            return Ok(content);
        }

        // 2. Default locale (skip if same as exact)
        if locale != default_locale {
            let path = self.path.join(default_locale).join(&filename);
            if let Some(content) = self.try_load(&path) {
                return Ok(content);
            }
        }

        // 3. No-locale fallback
        let path = self.path.join(&filename);
        if let Some(content) = self.try_load(&path) {
            return Ok(content);
        }

        // 4. Error
        Err(Error::not_found(format!(
            "email template '{name}' not found for locale '{locale}'"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_templates(dir: &std::path::Path) {
        // en/welcome.md
        std::fs::create_dir_all(dir.join("en")).unwrap();
        std::fs::write(
            dir.join("en/welcome.md"),
            "---\nsubject: Welcome EN\n---\nEnglish body",
        )
        .unwrap();

        // uk/welcome.md
        std::fs::create_dir_all(dir.join("uk")).unwrap();
        std::fs::write(
            dir.join("uk/welcome.md"),
            "---\nsubject: Welcome UK\n---\nUkrainian body",
        )
        .unwrap();

        // fallback.md (no locale dir)
        std::fs::write(
            dir.join("fallback.md"),
            "---\nsubject: Fallback\n---\nFallback body",
        )
        .unwrap();
    }

    #[test]
    fn load_exact_locale() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        let source = FileSource::new(dir.path());

        let content = source.load("welcome", "uk", "en").unwrap();
        assert!(content.contains("Ukrainian body"));
    }

    #[test]
    fn load_falls_back_to_default_locale() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        let source = FileSource::new(dir.path());

        let content = source.load("welcome", "fr", "en").unwrap();
        assert!(content.contains("English body"));
    }

    #[test]
    fn load_falls_back_to_no_locale() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        let source = FileSource::new(dir.path());

        let content = source.load("fallback", "fr", "en").unwrap();
        assert!(content.contains("Fallback body"));
    }

    #[test]
    fn load_not_found() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        let source = FileSource::new(dir.path());

        let result = source.load("nonexistent", "en", "en");
        assert!(result.is_err());
    }

    #[test]
    fn load_same_locale_as_default_skips_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        setup_templates(dir.path());
        let source = FileSource::new(dir.path());

        // locale == default_locale, should still find it on first try
        let content = source.load("welcome", "en", "en").unwrap();
        assert!(content.contains("English body"));
    }
}
```

- [ ] **Step 2: Add to mod.rs**

Add to `src/email/mod.rs`:

```rust
mod source;

pub use source::{FileSource, TemplateSource};
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features email --lib -- email::source::tests -v`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/email/source.rs src/email/mod.rs
git commit -m "feat(email): add TemplateSource trait and FileSource with locale fallback"
```

---

### Task 10: CachedSource wrapper

**Files:**
- Create: `src/email/cache.rs`
- Modify: `src/email/mod.rs`

- [ ] **Step 1: Write cache module with tests**

Create `src/email/cache.rs`:

```rust
use crate::Result;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

use crate::email::source::TemplateSource;

/// LRU-cached wrapper around any `TemplateSource`.
/// Cache key is `(name, locale)`.
pub struct CachedSource<S: TemplateSource> {
    inner: S,
    cache: Mutex<LruCache<(String, String), String>>,
}

impl<S: TemplateSource> CachedSource<S> {
    pub fn new(inner: S, capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1).unwrap());
        Self {
            inner,
            cache: Mutex::new(LruCache::new(cap)),
        }
    }
}

impl<S: TemplateSource> TemplateSource for CachedSource<S> {
    fn load(&self, name: &str, locale: &str, default_locale: &str) -> Result<String> {
        let key = (name.to_string(), locale.to_string());

        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(&key) {
                return Ok(cached.clone());
            }
        }

        let content = self.inner.load(name, locale, default_locale)?;

        {
            let mut cache = self.cache.lock().unwrap();
            cache.put(key, content.clone());
        }

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A mock source that counts how many times load() is called.
    struct CountingSource {
        calls: Arc<AtomicUsize>,
        templates: HashMap<String, String>,
    }

    impl CountingSource {
        fn new(templates: HashMap<String, String>) -> (Self, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    calls: calls.clone(),
                    templates,
                },
                calls,
            )
        }
    }

    impl TemplateSource for CountingSource {
        fn load(&self, name: &str, _locale: &str, _default_locale: &str) -> Result<String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.templates
                .get(name)
                .cloned()
                .ok_or_else(|| crate::Error::not_found(format!("not found: {name}")))
        }
    }

    #[test]
    fn cache_hit_avoids_inner_call() {
        let mut templates = HashMap::new();
        templates.insert("welcome".into(), "content".into());
        let (source, calls) = CountingSource::new(templates);
        let cached = CachedSource::new(source, 10);

        // First load — cache miss
        let result = cached.load("welcome", "en", "en").unwrap();
        assert_eq!(result, "content");
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Second load — cache hit
        let result = cached.load("welcome", "en", "en").unwrap();
        assert_eq!(result, "content");
        assert_eq!(calls.load(Ordering::SeqCst), 1); // not incremented
    }

    #[test]
    fn cache_different_locales_are_separate_entries() {
        let mut templates = HashMap::new();
        templates.insert("welcome".into(), "content".into());
        let (source, calls) = CountingSource::new(templates);
        let cached = CachedSource::new(source, 10);

        cached.load("welcome", "en", "en").unwrap();
        cached.load("welcome", "uk", "en").unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn cache_eviction_on_capacity() {
        let mut templates = HashMap::new();
        templates.insert("a".into(), "content_a".into());
        templates.insert("b".into(), "content_b".into());
        let (source, calls) = CountingSource::new(templates);
        let cached = CachedSource::new(source, 1); // capacity of 1

        cached.load("a", "en", "en").unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        cached.load("b", "en", "en").unwrap(); // evicts "a"
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        cached.load("a", "en", "en").unwrap(); // cache miss again
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn cache_propagates_errors() {
        let templates = HashMap::new();
        let (source, _) = CountingSource::new(templates);
        let cached = CachedSource::new(source, 10);

        let result = cached.load("missing", "en", "en");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Add to mod.rs**

Add to `src/email/mod.rs`:

```rust
mod cache;

pub use cache::CachedSource;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --features email --lib -- email::cache::tests -v`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/email/cache.rs src/email/mod.rs
git commit -m "feat(email): add CachedSource LRU wrapper for TemplateSource"
```

---

### Task 11: Mailer struct — construction, render, send

**Files:**
- Create: `src/email/mailer.rs`
- Modify: `src/email/mod.rs`

- [ ] **Step 1: Write Mailer with Transport enum and construction**

Create `src/email/mailer.rs`:

```rust
use crate::email::cache::CachedSource;
use crate::email::layout;
use crate::email::markdown;
use crate::email::message::{RenderedEmail, SendEmail};
use crate::email::render;
use crate::email::source::{FileSource, TemplateSource};
use crate::{Error, Result};
use lettre::message::{header::ContentType, MultiPart, SinglePart};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::collections::HashMap;
use std::sync::Arc;

use crate::email::config::{EmailConfig, SmtpSecurity};

enum Transport {
    Smtp(AsyncSmtpTransport<Tokio1Executor>),
    #[cfg(feature = "email-test")]
    Stub(lettre::transport::stub::AsyncStubTransport),
}

pub struct Mailer {
    source: Arc<dyn TemplateSource>,
    transport: Transport,
    config: EmailConfig,
    layouts: HashMap<String, String>,
}

impl Mailer {
    /// Create a new Mailer with the default FileSource (cached if configured).
    pub fn new(config: &EmailConfig) -> Result<Self> {
        let file_source = FileSource::new(&config.templates_path);
        let source: Arc<dyn TemplateSource> = if config.cache_templates {
            Arc::new(CachedSource::new(file_source, config.template_cache_size))
        } else {
            Arc::new(file_source)
        };

        let transport = Self::build_smtp_transport(config)?;
        let layouts = layout::load_layouts(&config.layouts_path)?;

        Ok(Self {
            source,
            transport: Transport::Smtp(transport),
            config: config.clone(),
            layouts,
        })
    }

    /// Create a new Mailer with a custom TemplateSource.
    pub fn with_source(
        config: &EmailConfig,
        source: Arc<dyn TemplateSource>,
    ) -> Result<Self> {
        let transport = Self::build_smtp_transport(config)?;
        let layouts = layout::load_layouts(&config.layouts_path)?;

        Ok(Self {
            source,
            transport: Transport::Smtp(transport),
            config: config.clone(),
            layouts,
        })
    }

    /// Create a Mailer with a stub transport for testing.
    #[cfg(feature = "email-test")]
    pub fn with_stub_transport(
        config: &EmailConfig,
        stub: lettre::transport::stub::AsyncStubTransport,
    ) -> Result<Self> {
        let file_source = FileSource::new(&config.templates_path);
        let source: Arc<dyn TemplateSource> = if config.cache_templates {
            Arc::new(CachedSource::new(file_source, config.template_cache_size))
        } else {
            Arc::new(file_source)
        };
        let layouts = layout::load_layouts(&config.layouts_path)?;

        Ok(Self {
            source,
            transport: Transport::Stub(stub),
            config: config.clone(),
            layouts,
        })
    }

    fn build_smtp_transport(config: &EmailConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
        // Validate SMTP auth: both set or both empty
        match (&config.smtp.username, &config.smtp.password) {
            (Some(_), None) | (None, Some(_)) => {
                return Err(Error::bad_request(
                    "SMTP username and password must both be set or both be empty",
                ));
            }
            _ => {}
        }

        let builder = match config.smtp.security {
            SmtpSecurity::Tls => {
                AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp.host)
                    .map_err(|e| Error::internal(format!("SMTP relay error: {e}")))?
            }
            SmtpSecurity::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp.host)
                    .map_err(|e| Error::internal(format!("SMTP STARTTLS error: {e}")))?
            }
            SmtpSecurity::None => {
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp.host)
            }
        };

        let builder = builder.port(config.smtp.port);

        let builder = if let (Some(username), Some(password)) =
            (&config.smtp.username, &config.smtp.password)
        {
            builder.credentials(lettre::transport::smtp::authentication::Credentials::new(
                username.clone(),
                password.clone(),
            ))
        } else {
            builder
        };

        Ok(builder.build())
    }

    /// Render a template without sending.
    pub fn render(&self, email: &SendEmail) -> Result<RenderedEmail> {
        let locale = email
            .locale
            .as_deref()
            .unwrap_or(&self.config.default_locale);

        // Load raw template
        let raw = self
            .source
            .load(&email.template, locale, &self.config.default_locale)?;

        // Stage 1: Substitute variables
        let substituted = render::substitute(&raw, &email.vars);

        // Stage 2: Parse frontmatter
        let (frontmatter, body) = render::parse_frontmatter(&substituted)?;

        // Stage 3: Render markdown to HTML
        let brand_color = email.vars.get("brand_color").map(|s| s.as_str());
        let html_body = markdown::markdown_to_html(body, brand_color);

        // Stage 4: Apply layout
        let layout_html = layout::resolve_layout(&frontmatter.layout, &self.layouts)?;
        let html = layout::apply_layout(&layout_html, &html_body, &email.vars);

        // Stage 5: Plain text
        let text = markdown::markdown_to_text(body);

        Ok(RenderedEmail {
            subject: frontmatter.subject,
            html,
            text,
        })
    }

    /// Render and send an email via SMTP.
    pub async fn send(&self, email: SendEmail) -> Result<()> {
        if email.to.is_empty() {
            return Err(Error::bad_request("email has no recipients"));
        }

        let rendered = self.render(&email)?;

        // Build sender
        let from_name = email
            .sender
            .as_ref()
            .map(|s| &s.from_name)
            .unwrap_or(&self.config.default_from_name);
        let from_email = email
            .sender
            .as_ref()
            .map(|s| &s.from_email)
            .unwrap_or(&self.config.default_from_email);
        let reply_to = email
            .sender
            .as_ref()
            .and_then(|s| s.reply_to.as_deref())
            .or(self.config.default_reply_to.as_deref());

        let from = if from_name.is_empty() {
            from_email.parse()
        } else {
            format!("{from_name} <{from_email}>").parse()
        }
        .map_err(|e| Error::bad_request(format!("invalid from address: {e}")))?;

        let mut builder = Message::builder()
            .from(from)
            .subject(&rendered.subject);

        for to_addr in &email.to {
            builder = builder.to(to_addr
                .parse()
                .map_err(|e| Error::bad_request(format!("invalid to address '{to_addr}': {e}")))?);
        }

        for cc_addr in &email.cc {
            builder = builder.cc(cc_addr
                .parse()
                .map_err(|e| Error::bad_request(format!("invalid cc address '{cc_addr}': {e}")))?);
        }

        for bcc_addr in &email.bcc {
            builder = builder.bcc(bcc_addr
                .parse()
                .map_err(|e| {
                    Error::bad_request(format!("invalid bcc address '{bcc_addr}': {e}"))
                })?);
        }

        if let Some(reply_to_addr) = reply_to {
            builder = builder.reply_to(
                reply_to_addr
                    .parse()
                    .map_err(|e| Error::bad_request(format!("invalid reply-to address: {e}")))?,
            );
        }

        let message = builder
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(rendered.text),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(rendered.html),
                    ),
            )
            .map_err(|e| Error::internal(format!("failed to build email message: {e}")))?;

        match &self.transport {
            Transport::Smtp(transport) => {
                transport
                    .send(message)
                    .await
                    .map_err(|e| Error::internal(format!("failed to send email: {e}")))?;
            }
            #[cfg(feature = "email-test")]
            Transport::Stub(transport) => {
                transport
                    .send(message)
                    .await
                    .map_err(|e| Error::internal(format!("failed to send email (stub): {e}")))?;
            }
        }

        Ok(())
    }
}
```

- [ ] **Step 2: Update mod.rs with final exports**

Replace `src/email/mod.rs` entirely:

```rust
mod button;
mod cache;
mod config;
mod layout;
mod mailer;
mod markdown;
mod message;
mod render;
mod source;

pub use button::ButtonType;
pub use cache::CachedSource;
pub use config::{EmailConfig, SmtpConfig, SmtpSecurity};
pub use mailer::Mailer;
pub use message::{RenderedEmail, SendEmail, SenderProfile};
pub use source::{FileSource, TemplateSource};
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check --features email`
Expected: compiles with no errors.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --features email --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add src/email/mailer.rs src/email/mod.rs
git commit -m "feat(email): add Mailer with render and send, Transport enum, SMTP construction"
```

---

### Task 12: Integration tests

**Files:**
- Create: `tests/email_test.rs`

- [ ] **Step 1: Create test template files**

Create a test fixture directory. The integration test will use `tempfile` to create templates on the fly, so no permanent fixtures are needed.

- [ ] **Step 2: Write integration tests**

Create `tests/email_test.rs`:

```rust
#![cfg(feature = "email-test")]

use modo::email::{EmailConfig, Mailer, SendEmail};
use std::collections::HashMap;

fn test_config(dir: &std::path::Path) -> EmailConfig {
    EmailConfig {
        templates_path: dir.to_str().unwrap().into(),
        layouts_path: dir.join("layouts").to_str().unwrap().into(),
        default_from_name: "TestApp".into(),
        default_from_email: "noreply@test.com".into(),
        default_reply_to: Some("support@test.com".into()),
        default_locale: "en".into(),
        cache_templates: false,
        ..EmailConfig::default()
    }
}

fn write_template(dir: &std::path::Path, locale: &str, name: &str, content: &str) {
    let locale_dir = dir.join(locale);
    std::fs::create_dir_all(&locale_dir).unwrap();
    std::fs::write(locale_dir.join(format!("{name}.md")), content).unwrap();
}

#[test]
fn render_basic_template() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "welcome",
        "---\nsubject: \"Welcome {{name}}!\"\n---\nHi **{{name}}**, welcome!",
    );

    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    let email = SendEmail::new("welcome", "user@example.com")
        .var("name", "Dmytro");

    let rendered = mailer.render(&email).unwrap();
    assert_eq!(rendered.subject, "Welcome Dmytro!");
    assert!(rendered.html.contains("<strong>Dmytro</strong>"));
    assert!(rendered.html.contains("max-width: 600px")); // base layout applied
    assert!(rendered.text.contains("Hi Dmytro, welcome!"));
}

#[test]
fn render_with_button() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "action",
        "---\nsubject: Action needed\n---\n[button:danger|Delete](https://example.com/del)",
    );

    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    let email = SendEmail::new("action", "user@example.com");
    let rendered = mailer.render(&email).unwrap();
    assert!(rendered.html.contains("background-color: #dc2626")); // danger color
    assert!(rendered.text.contains("Delete: https://example.com/del"));
}

#[test]
fn render_with_custom_layout() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "custom",
        "---\nsubject: Custom\nlayout: simple\n---\nBody here",
    );
    std::fs::create_dir_all(dir.path().join("layouts")).unwrap();
    std::fs::write(
        dir.path().join("layouts/simple.html"),
        "<html><body>{{content}}</body></html>",
    )
    .unwrap();

    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    let email = SendEmail::new("custom", "user@example.com");
    let rendered = mailer.render(&email).unwrap();
    assert!(rendered.html.starts_with("<html>"));
    assert!(rendered.html.contains("Body here"));
    assert!(!rendered.html.contains("max-width: 600px")); // not base layout
}

#[test]
fn render_locale_fallback() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "greeting",
        "---\nsubject: English Greeting\n---\nHello!",
    );

    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    // Request French, falls back to English
    let email = SendEmail::new("greeting", "user@example.com").locale("fr");
    let rendered = mailer.render(&email).unwrap();
    assert_eq!(rendered.subject, "English Greeting");
}

#[tokio::test]
async fn send_with_stub_transport() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "welcome",
        "---\nsubject: Welcome!\n---\nHello!",
    );

    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub.clone()).unwrap();

    mailer
        .send(
            SendEmail::new("welcome", "user@example.com")
                .cc("cc@example.com")
                .bcc("bcc@example.com"),
        )
        .await
        .unwrap();

    let msgs = stub.messages().await;
    assert_eq!(msgs.len(), 1);
    let (envelope, raw) = &msgs[0];
    assert!(envelope
        .to()
        .iter()
        .any(|a| a.as_ref() == "user@example.com"));
    assert!(raw.contains("Subject: Welcome!"));
}

#[tokio::test]
async fn send_empty_to_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "test",
        "---\nsubject: Test\n---\nBody",
    );

    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    let email = SendEmail {
        template: "test".into(),
        to: vec![],
        cc: vec![],
        bcc: vec![],
        locale: None,
        vars: HashMap::new(),
        sender: None,
    };

    let result = mailer.send(email).await;
    assert!(result.is_err());
}

#[test]
fn render_template_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    let email = SendEmail::new("nonexistent", "user@example.com");
    let result = mailer.render(&email);
    assert!(result.is_err());
}
```

- [ ] **Step 3: Run integration tests**

Run: `cargo test --features email-test -- email_test -v`
Expected: all tests pass.

- [ ] **Step 4: Run full test suite and clippy**

Run: `cargo test --features email-test`
Run: `cargo clippy --features email --tests -- -D warnings`
Expected: all pass, no warnings.

- [ ] **Step 5: Commit**

```bash
git add tests/email_test.rs
git commit -m "test(email): add integration tests for Mailer render and send"
```

---

### Task 13: Final verification and cleanup

**Files:**
- Verify: all `src/email/*.rs` files
- Verify: `Cargo.toml`
- Verify: `src/lib.rs`
- Verify: `src/config/modo.rs`

- [ ] **Step 1: Run full test suite**

Run: `cargo test --features email-test`
Expected: all tests pass.

- [ ] **Step 2: Run clippy on all code including email**

Run: `cargo clippy --features email --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 3: Check formatting**

Run: `cargo fmt --check`
Expected: no formatting issues.

- [ ] **Step 4: Verify without email feature (no regression)**

Run: `cargo check`
Run: `cargo test`
Expected: compiles and all non-email tests pass.

- [ ] **Step 5: Verify with auth + email features together**

Run: `cargo check --features auth,email`
Expected: compiles with no errors.

- [ ] **Step 6: Commit any final fixes if needed**

```bash
git add -A
git commit -m "chore(email): final cleanup and verification"
```
