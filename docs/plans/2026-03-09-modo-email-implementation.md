# modo-email Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a pluggable email sending module with Markdown templates, responsive HTML output, and multi-tenant sender support.

**Architecture:** `Mailer` service wraps a `MailTransport` trait (SMTP/Resend) and `TemplateProvider` trait (filesystem default). Markdown templates with `{{var}}` substitution are rendered to HTML via `pulldown-cmark`, wrapped in a MiniJinja layout, and delivered through the configured transport.

**Tech Stack:** pulldown-cmark (Markdown), minijinja (layouts), lettre (SMTP), reqwest (Resend HTTP API), serde_yaml_ng (frontmatter), async-trait (object-safe async traits)

**Design doc:** `docs/plans/2026-03-09-modo-email-design.md`

---

### Task 1: Scaffold Crate

**Files:**
- Create: `modo-email/Cargo.toml`
- Create: `modo-email/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "modo-email"
version = "0.1.0"
edition = "2024"
license.workspace = true

[features]
default = ["smtp"]
smtp = ["dep:lettre"]
resend = ["dep:reqwest"]

[dependencies]
modo = { path = "../modo" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml_ng = "0.10"
async-trait = "0.1"
pulldown-cmark = "0.12"
minijinja = { version = "2", features = ["loader"] }
tracing = "0.1"
lettre = { version = "0.11", features = ["tokio1-native-tls", "builder", "smtp-transport"], optional = true }
reqwest = { version = "0.12", features = ["json"], optional = true }

[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
tempfile = "3"
```

**Step 2: Create src/lib.rs with module stubs**

```rust
mod config;
mod mailer;
mod message;
mod template;
mod transport;

pub use config::{EmailConfig, TransportBackend};
pub use mailer::Mailer;
pub use message::{MailMessage, SendEmail, SendEmailPayload, SenderProfile};
pub use template::{EmailTemplate, TemplateProvider};
pub use transport::MailTransport;

#[cfg(feature = "smtp")]
pub use config::SmtpConfig;
#[cfg(feature = "resend")]
pub use config::ResendConfig;
```

Create empty module files:
- `modo-email/src/config.rs`
- `modo-email/src/mailer.rs`
- `modo-email/src/message.rs`
- `modo-email/src/template/mod.rs`
- `modo-email/src/template/filesystem.rs`
- `modo-email/src/template/markdown.rs`
- `modo-email/src/template/layout.rs`
- `modo-email/src/transport/mod.rs`
- `modo-email/src/transport/smtp.rs`
- `modo-email/src/transport/resend.rs`

**Step 3: Add to workspace**

Add `"modo-email"` to the `members` list in the root `Cargo.toml`.

**Step 4: Verify it compiles**

Run: `cargo check -p modo-email`
Expected: PASS (empty modules)

**Step 5: Commit**

```bash
git add modo-email/ Cargo.toml
git commit -m "chore: scaffold modo-email crate"
```

---

### Task 2: Config Types

**Files:**
- Modify: `modo-email/src/config.rs`

**Step 1: Write tests for config**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let config = EmailConfig::default();
        assert_eq!(config.templates_path, "emails");
        assert_eq!(config.default_from_name, "");
        assert_eq!(config.default_from_email, "");
        assert!(config.default_reply_to.is_none());
        assert_eq!(config.transport, TransportBackend::Smtp);
    }

    #[test]
    fn partial_yaml_deserialization() {
        let yaml = r#"
templates_path: "mail"
default_from_name: "Acme"
default_from_email: "hi@acme.com"
"#;
        let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.templates_path, "mail");
        assert_eq!(config.default_from_name, "Acme");
        assert_eq!(config.default_from_email, "hi@acme.com");
        assert_eq!(config.transport, TransportBackend::Smtp);
    }

    #[test]
    fn transport_backend_deserialization() {
        let yaml = "transport: resend";
        let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(config.transport, TransportBackend::Resend);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-email`
Expected: FAIL — types don't exist

**Step 3: Implement config types**

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransportBackend {
    #[default]
    Smtp,
    Resend,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    pub transport: TransportBackend,
    pub templates_path: String,
    pub default_from_name: String,
    pub default_from_email: String,
    pub default_reply_to: Option<String>,

    #[cfg(feature = "smtp")]
    pub smtp: SmtpConfig,

    #[cfg(feature = "resend")]
    pub resend: ResendConfig,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            transport: TransportBackend::default(),
            templates_path: "emails".to_string(),
            default_from_name: String::new(),
            default_from_email: String::new(),
            default_reply_to: None,
            #[cfg(feature = "smtp")]
            smtp: SmtpConfig::default(),
            #[cfg(feature = "resend")]
            resend: ResendConfig::default(),
        }
    }
}

#[cfg(feature = "smtp")]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub tls: bool,
}

#[cfg(feature = "smtp")]
impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 587,
            username: String::new(),
            password: String::new(),
            tls: true,
        }
    }
}

#[cfg(feature = "resend")]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ResendConfig {
    pub api_key: String,
}

#[cfg(feature = "resend")]
impl Default for ResendConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
        }
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p modo-email`
Expected: PASS

**Step 5: Commit**

```bash
git add modo-email/src/config.rs
git commit -m "feat(modo-email): add config types"
```

---

### Task 3: Core Message Types

**Files:**
- Modify: `modo-email/src/message.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sender_profile_serialization_roundtrip() {
        let profile = SenderProfile {
            from_name: "Acme".to_string(),
            from_email: "hi@acme.com".to_string(),
            reply_to: Some("support@acme.com".to_string()),
        };
        let json = serde_json::to_string(&profile).unwrap();
        let back: SenderProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.from_name, "Acme");
        assert_eq!(back.reply_to, Some("support@acme.com".to_string()));
    }

    #[test]
    fn sender_profile_format_address() {
        let profile = SenderProfile {
            from_name: "Acme Corp".to_string(),
            from_email: "hi@acme.com".to_string(),
            reply_to: None,
        };
        assert_eq!(profile.format_address(), "Acme Corp <hi@acme.com>");
    }

    #[test]
    fn send_email_builder() {
        let email = SendEmail::new("welcome", "user@test.com")
            .locale("de")
            .var("name", "Hans")
            .var("code", "1234");
        assert_eq!(email.template, "welcome");
        assert_eq!(email.to, "user@test.com");
        assert_eq!(email.locale.as_deref(), Some("de"));
        assert_eq!(email.context.len(), 2);
    }

    #[test]
    fn send_email_context_merge() {
        let mut brand = HashMap::new();
        brand.insert("logo".to_string(), serde_json::json!("https://logo.png"));
        brand.insert("color".to_string(), serde_json::json!("#ff0000"));

        let email = SendEmail::new("welcome", "u@t.com")
            .context(&brand)
            .var("name", "Alice");
        assert_eq!(email.context.len(), 3);
    }

    #[test]
    fn payload_roundtrip() {
        let email = SendEmail::new("welcome", "u@t.com")
            .locale("en")
            .var("name", "Alice");
        let payload = SendEmailPayload::from(email);
        let json = serde_json::to_string(&payload).unwrap();
        let back: SendEmailPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back.template, "welcome");
        assert_eq!(back.locale.as_deref(), Some("en"));

        let email_back = SendEmail::from(back);
        assert_eq!(email_back.to, "u@t.com");
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-email`
Expected: FAIL

**Step 3: Implement types**

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderProfile {
    pub from_name: String,
    pub from_email: String,
    pub reply_to: Option<String>,
}

impl SenderProfile {
    pub fn format_address(&self) -> String {
        format!("{} <{}>", self.from_name, self.from_email)
    }
}

pub struct MailMessage {
    pub from: String,
    pub reply_to: Option<String>,
    pub to: String,
    pub subject: String,
    pub html: String,
    pub text: String,
}

pub struct SendEmail {
    pub(crate) template: String,
    pub(crate) to: String,
    pub(crate) locale: Option<String>,
    pub(crate) sender: Option<SenderProfile>,
    pub(crate) context: HashMap<String, serde_json::Value>,
}

impl SendEmail {
    pub fn new(template: &str, to: &str) -> Self {
        Self {
            template: template.to_string(),
            to: to.to_string(),
            locale: None,
            sender: None,
            context: HashMap::new(),
        }
    }

    pub fn locale(mut self, locale: &str) -> Self {
        self.locale = Some(locale.to_string());
        self
    }

    pub fn sender(mut self, sender: &SenderProfile) -> Self {
        self.sender = Some(sender.clone());
        self
    }

    pub fn var(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        self.context.insert(key.to_string(), value.into());
        self
    }

    pub fn context(mut self, ctx: &HashMap<String, serde_json::Value>) -> Self {
        self.context.extend(ctx.clone());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendEmailPayload {
    pub template: String,
    pub to: String,
    pub locale: Option<String>,
    pub sender: Option<SenderProfile>,
    pub context: HashMap<String, serde_json::Value>,
}

impl From<SendEmail> for SendEmailPayload {
    fn from(e: SendEmail) -> Self {
        Self {
            template: e.template,
            to: e.to,
            locale: e.locale,
            sender: e.sender,
            context: e.context,
        }
    }
}

impl From<SendEmailPayload> for SendEmail {
    fn from(p: SendEmailPayload) -> Self {
        Self {
            template: p.template,
            to: p.to,
            locale: p.locale,
            sender: p.sender,
            context: p.context,
        }
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p modo-email`
Expected: PASS

**Step 5: Commit**

```bash
git add modo-email/src/message.rs
git commit -m "feat(modo-email): add message types, builder, and payload"
```

---

### Task 4: MailTransport Trait + SMTP Backend

**Files:**
- Modify: `modo-email/src/transport/mod.rs`
- Modify: `modo-email/src/transport/smtp.rs`

**Step 1: Define the trait**

In `transport/mod.rs`:

```rust
#[cfg(feature = "smtp")]
pub mod smtp;
#[cfg(feature = "resend")]
pub mod resend;

use crate::message::MailMessage;

#[async_trait::async_trait]
pub trait MailTransport: Send + Sync + 'static {
    async fn send(&self, message: &MailMessage) -> Result<(), modo::Error>;
}
```

**Step 2: Implement SMTP transport**

In `transport/smtp.rs`:

```rust
use super::MailTransport;
use crate::config::SmtpConfig;
use crate::message::MailMessage;
use lettre::message::{header::ContentType, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

pub struct SmtpTransport {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpTransport {
    pub fn new(config: &SmtpConfig) -> Result<Self, modo::Error> {
        let creds = Credentials::new(config.username.clone(), config.password.clone());

        let builder = if config.tls {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
                .port(config.port)
                .into()  // adjust as needed per lettre API
        };

        let mailer = builder
            .map_err(|e| modo::Error::internal(format!("SMTP config error: {e}")))?
            .credentials(creds)
            .port(config.port)
            .build();

        Ok(Self { mailer })
    }
}

#[async_trait::async_trait]
impl MailTransport for SmtpTransport {
    async fn send(&self, message: &MailMessage) -> Result<(), modo::Error> {
        let mut builder = Message::builder()
            .from(message.from.parse().map_err(|e| {
                modo::Error::internal(format!("Invalid from address: {e}"))
            })?)
            .to(message.to.parse().map_err(|e| {
                modo::Error::internal(format!("Invalid to address: {e}"))
            })?)
            .subject(&message.subject);

        if let Some(ref reply_to) = message.reply_to {
            builder = builder.reply_to(reply_to.parse().map_err(|e| {
                modo::Error::internal(format!("Invalid reply-to address: {e}"))
            })?);
        }

        let email = builder
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(message.text.clone()),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(message.html.clone()),
                    ),
            )
            .map_err(|e| modo::Error::internal(format!("Failed to build email: {e}")))?;

        self.mailer
            .send(email)
            .await
            .map_err(|e| modo::Error::internal(format!("SMTP send failed: {e}")))?;

        Ok(())
    }
}
```

**Step 3: Verify it compiles**

Run: `cargo check -p modo-email`
Expected: PASS

Note: SMTP transport is tested via integration tests in Task 14. Unit-testing requires a real SMTP server or mock, which adds complexity. The trait boundary is the testable seam.

**Step 4: Commit**

```bash
git add modo-email/src/transport/
git commit -m "feat(modo-email): add MailTransport trait and SMTP backend"
```

---

### Task 5: Resend HTTP Transport

**Files:**
- Modify: `modo-email/src/transport/resend.rs`

**Step 1: Implement Resend transport**

```rust
use super::MailTransport;
use crate::config::ResendConfig;
use crate::message::MailMessage;

pub struct ResendTransport {
    client: reqwest::Client,
    api_key: String,
}

impl ResendTransport {
    pub fn new(config: &ResendConfig) -> Result<Self, modo::Error> {
        let client = reqwest::Client::new();
        Ok(Self {
            client,
            api_key: config.api_key.clone(),
        })
    }
}

#[async_trait::async_trait]
impl MailTransport for ResendTransport {
    async fn send(&self, message: &MailMessage) -> Result<(), modo::Error> {
        let mut body = serde_json::json!({
            "from": message.from,
            "to": [message.to],
            "subject": message.subject,
            "html": message.html,
            "text": message.text,
        });

        if let Some(ref reply_to) = message.reply_to {
            body["reply_to"] = serde_json::json!(reply_to);
        }

        let resp = self
            .client
            .post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| modo::Error::internal(format!("Resend request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(modo::Error::internal(format!(
                "Resend API error ({status}): {text}"
            )));
        }

        Ok(())
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo check -p modo-email --features resend`
Expected: PASS

**Step 3: Commit**

```bash
git add modo-email/src/transport/resend.rs
git commit -m "feat(modo-email): add Resend HTTP transport"
```

---

### Task 6: TemplateProvider Trait + EmailTemplate + Frontmatter Parsing

**Files:**
- Modify: `modo-email/src/template/mod.rs`

**Step 1: Write tests for frontmatter parsing**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_template_with_frontmatter() {
        let raw = "---\nsubject: \"Hello {{name}}\"\nlayout: custom\n---\n\nBody here.";
        let tpl = EmailTemplate::parse(raw).unwrap();
        assert_eq!(tpl.subject, "Hello {{name}}");
        assert_eq!(tpl.layout.as_deref(), Some("custom"));
        assert_eq!(tpl.body.trim(), "Body here.");
    }

    #[test]
    fn parse_template_default_layout() {
        let raw = "---\nsubject: \"Hi\"\n---\n\nContent.";
        let tpl = EmailTemplate::parse(raw).unwrap();
        assert_eq!(tpl.subject, "Hi");
        assert!(tpl.layout.is_none());
    }

    #[test]
    fn parse_template_missing_subject() {
        let raw = "---\nlayout: default\n---\n\nNo subject.";
        let result = EmailTemplate::parse(raw);
        assert!(result.is_err());
    }

    #[test]
    fn parse_template_no_frontmatter() {
        let raw = "Just markdown, no frontmatter.";
        let result = EmailTemplate::parse(raw);
        assert!(result.is_err());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-email`
Expected: FAIL

**Step 3: Implement**

```rust
pub mod filesystem;
pub mod layout;
pub mod markdown;

use serde::Deserialize;

pub struct EmailTemplate {
    pub subject: String,
    pub body: String,
    pub layout: Option<String>,
}

#[derive(Deserialize)]
struct Frontmatter {
    subject: String,
    layout: Option<String>,
}

impl EmailTemplate {
    pub fn parse(raw: &str) -> Result<Self, modo::Error> {
        let raw = raw.trim();
        if !raw.starts_with("---") {
            return Err(modo::Error::internal(
                "Email template must start with YAML frontmatter (---)",
            ));
        }

        let after_first = &raw[3..];
        let end = after_first.find("---").ok_or_else(|| {
            modo::Error::internal("Email template frontmatter missing closing ---")
        })?;

        let yaml = &after_first[..end];
        let body = &after_first[end + 3..];

        let fm: Frontmatter = serde_yaml_ng::from_str(yaml)
            .map_err(|e| modo::Error::internal(format!("Invalid frontmatter: {e}")))?;

        Ok(Self {
            subject: fm.subject,
            body: body.to_string(),
            layout: fm.layout,
        })
    }
}

pub trait TemplateProvider: Send + Sync + 'static {
    fn get(&self, name: &str, locale: &str) -> Result<EmailTemplate, modo::Error>;
}
```

**Step 4: Run tests**

Run: `cargo test -p modo-email`
Expected: PASS

**Step 5: Commit**

```bash
git add modo-email/src/template/mod.rs
git commit -m "feat(modo-email): add TemplateProvider trait and frontmatter parsing"
```

---

### Task 7: FilesystemProvider

**Files:**
- Modify: `modo-email/src/template/filesystem.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::TemplateProvider;
    use std::fs;

    #[test]
    fn load_template_no_locale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        fs::write(
            path.join("welcome.md"),
            "---\nsubject: \"Hi\"\n---\n\nHello!",
        ).unwrap();

        let provider = FilesystemProvider::new(path.to_str().unwrap());
        let tpl = provider.get("welcome", "").unwrap();
        assert_eq!(tpl.subject, "Hi");
    }

    #[test]
    fn load_template_with_locale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        fs::create_dir_all(path.join("de")).unwrap();
        fs::write(
            path.join("de/welcome.md"),
            "---\nsubject: \"Hallo\"\n---\n\nHallo!",
        ).unwrap();
        fs::write(
            path.join("welcome.md"),
            "---\nsubject: \"Hi\"\n---\n\nHello!",
        ).unwrap();

        let provider = FilesystemProvider::new(path.to_str().unwrap());
        let tpl = provider.get("welcome", "de").unwrap();
        assert_eq!(tpl.subject, "Hallo");
    }

    #[test]
    fn locale_fallback_to_root() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        fs::write(
            path.join("welcome.md"),
            "---\nsubject: \"Hi\"\n---\n\nHello!",
        ).unwrap();

        let provider = FilesystemProvider::new(path.to_str().unwrap());
        let tpl = provider.get("welcome", "fr").unwrap();
        assert_eq!(tpl.subject, "Hi");
    }

    #[test]
    fn template_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FilesystemProvider::new(dir.path().to_str().unwrap());
        let result = provider.get("missing", "");
        assert!(result.is_err());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-email`
Expected: FAIL

**Step 3: Implement**

```rust
use super::{EmailTemplate, TemplateProvider};
use std::path::{Path, PathBuf};

pub struct FilesystemProvider {
    base_dir: PathBuf,
}

impl FilesystemProvider {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    fn resolve_path(&self, name: &str, locale: &str) -> Option<PathBuf> {
        // 1. Try {base}/{locale}/{name}.md
        if !locale.is_empty() {
            let localized = self.base_dir.join(locale).join(format!("{name}.md"));
            if localized.is_file() {
                return Some(localized);
            }
        }

        // 2. Try {base}/{name}.md
        let root = self.base_dir.join(format!("{name}.md"));
        if root.is_file() {
            return Some(root);
        }

        None
    }
}

impl TemplateProvider for FilesystemProvider {
    fn get(&self, name: &str, locale: &str) -> Result<EmailTemplate, modo::Error> {
        let path = self.resolve_path(name, locale).ok_or_else(|| {
            modo::Error::internal(format!("Email template not found: {name}"))
        })?;

        let raw = std::fs::read_to_string(&path).map_err(|e| {
            modo::Error::internal(format!("Failed to read template {}: {e}", path.display()))
        })?;

        EmailTemplate::parse(&raw)
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p modo-email`
Expected: PASS

**Step 5: Commit**

```bash
git add modo-email/src/template/filesystem.rs
git commit -m "feat(modo-email): add filesystem template provider with locale fallback"
```

---

### Task 8: Variable Substitution

**Files:**
- Create: `modo-email/src/template/vars.rs`
- Modify: `modo-email/src/template/mod.rs` (add `pub mod vars;`)

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn replace_simple_vars() {
        let mut ctx = HashMap::new();
        ctx.insert("name".to_string(), json!("Alice"));
        ctx.insert("code".to_string(), json!("1234"));

        let result = substitute("Hello {{name}}, code: {{code}}", &ctx);
        assert_eq!(result, "Hello Alice, code: 1234");
    }

    #[test]
    fn unresolved_vars_left_as_is() {
        let ctx = HashMap::new();
        let result = substitute("Hello {{name}}", &ctx);
        assert_eq!(result, "Hello {{name}}");
    }

    #[test]
    fn whitespace_in_braces() {
        let mut ctx = HashMap::new();
        ctx.insert("name".to_string(), json!("Bob"));
        let result = substitute("Hello {{ name }}", &ctx);
        assert_eq!(result, "Hello Bob");
    }

    #[test]
    fn non_string_values() {
        let mut ctx = HashMap::new();
        ctx.insert("count".to_string(), json!(42));
        ctx.insert("active".to_string(), json!(true));
        let result = substitute("Count: {{count}}, active: {{active}}", &ctx);
        assert_eq!(result, "Count: 42, active: true");
    }

    #[test]
    fn empty_input() {
        let ctx = HashMap::new();
        assert_eq!(substitute("", &ctx), "");
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-email`
Expected: FAIL

**Step 3: Implement**

```rust
use std::collections::HashMap;

pub fn substitute(input: &str, context: &HashMap<String, serde_json::Value>) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' && chars.peek() == Some(&'{') {
            chars.next(); // consume second {
            let mut key = String::new();
            let mut found_close = false;

            for ch in chars.by_ref() {
                if ch == '}' {
                    // Check for second }
                    if chars.peek() == Some(&'}') {
                        chars.next();
                        found_close = true;
                        break;
                    }
                    key.push(ch);
                    continue;
                }
                key.push(ch);
            }

            let key = key.trim();
            if found_close {
                if let Some(val) = context.get(key) {
                    match val {
                        serde_json::Value::String(s) => result.push_str(s),
                        other => result.push_str(&other.to_string()),
                    }
                } else {
                    result.push_str("{{");
                    result.push_str(key);
                    result.push_str("}}");
                }
            } else {
                result.push_str("{{");
                result.push_str(key);
            }
        } else {
            result.push(ch);
        }
    }

    result
}
```

**Step 4: Run tests**

Run: `cargo test -p modo-email`
Expected: PASS

**Step 5: Commit**

```bash
git add modo-email/src/template/vars.rs modo-email/src/template/mod.rs
git commit -m "feat(modo-email): add template variable substitution"
```

---

### Task 9: Markdown Renderer with Button Support

**Files:**
- Modify: `modo-email/src/template/markdown.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_basic_markdown() {
        let html = render_markdown("Hello **world**");
        assert!(html.contains("<strong>world</strong>"));
    }

    #[test]
    fn render_link_as_link() {
        let html = render_markdown("[Click](https://example.com)");
        assert!(html.contains("<a"));
        assert!(html.contains("href=\"https://example.com\""));
        assert!(html.contains("Click"));
    }

    #[test]
    fn render_button_link() {
        let html = render_markdown("[button|Get Started](https://example.com)");
        assert!(html.contains("Get Started"));
        assert!(html.contains("https://example.com"));
        assert!(html.contains("role=\"presentation\""));
        assert!(!html.contains("button|"));
    }

    #[test]
    fn render_normal_link_with_pipe() {
        let html = render_markdown("[some|text](https://example.com)");
        // "some" is not a known element type, render as normal link
        assert!(html.contains("some|text"));
        assert!(html.contains("<a"));
    }

    #[test]
    fn plain_text_from_markdown() {
        let text = render_plain_text("Hello **world**\n\n[button|Click](https://url.com)\n\n[Link](https://other.com)");
        assert!(text.contains("Hello world"));
        assert!(text.contains("Click (https://url.com)"));
        assert!(text.contains("Link (https://other.com)"));
        assert!(!text.contains("button|"));
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-email`
Expected: FAIL

**Step 3: Implement**

```rust
use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};

const BUTTON_PREFIX: &str = "button|";

const DEFAULT_BUTTON_COLOR: &str = "#4F46E5";

pub fn render_markdown(markdown: &str) -> String {
    render_markdown_with_color(markdown, DEFAULT_BUTTON_COLOR)
}

pub fn render_markdown_with_color(markdown: &str, button_color: &str) -> String {
    let opts = Options::empty();
    let parser = Parser::new_ext(markdown, opts);

    let mut html = String::new();
    let mut in_button_link = false;
    let mut button_url = String::new();
    let mut button_text = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Link { link_type: LinkType::Inline, dest_url, .. }) => {
                // We don't know yet if it's a button — buffer until we see text
                button_url = dest_url.to_string();
                in_button_link = true;
                button_text.clear();
            }
            Event::Text(text) if in_button_link => {
                button_text.push_str(&text);
            }
            Event::End(TagEnd::Link) if in_button_link => {
                in_button_link = false;
                if button_text.starts_with(BUTTON_PREFIX) {
                    let label = &button_text[BUTTON_PREFIX.len()..];
                    html.push_str(&render_button(label, &button_url, button_color));
                } else {
                    // Normal link
                    html.push_str(&format!(
                        "<a href=\"{}\" style=\"color:{button_color}\">{}</a>",
                        button_url, button_text,
                    ));
                }
            }
            _ if in_button_link => {
                // Non-text event inside a link — treat as normal link
                // (shouldn't happen with simple markdown, but handle gracefully)
            }
            _ => {
                // Default: use pulldown-cmark's HTML push
                pulldown_cmark::html::push_html(&mut html, std::iter::once(event));
            }
        }
    }

    html
}

fn render_button(label: &str, url: &str, color: &str) -> String {
    format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:16px auto"><tr><td style="background-color:{color};border-radius:6px;padding:12px 24px;text-align:center"><a href="{url}" style="color:#ffffff;text-decoration:none;font-weight:bold;font-size:16px;display:inline-block">{label}</a></td></tr></table>"#,
    )
}

pub fn render_plain_text(markdown: &str) -> String {
    let opts = Options::empty();
    let parser = Parser::new_ext(markdown, opts);

    let mut text = String::new();
    let mut in_link = false;
    let mut link_text = String::new();
    let mut link_url = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                in_link = true;
                link_url = dest_url.to_string();
                link_text.clear();
            }
            Event::Text(t) if in_link => {
                link_text.push_str(&t);
            }
            Event::End(TagEnd::Link) => {
                in_link = false;
                let display = if link_text.starts_with(BUTTON_PREFIX) {
                    &link_text[BUTTON_PREFIX.len()..]
                } else {
                    &link_text
                };
                text.push_str(&format!("{display} ({link_url})"));
            }
            Event::Text(t) => text.push_str(&t),
            Event::SoftBreak | Event::HardBreak => text.push('\n'),
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => text.push_str("\n\n"),
            Event::Start(Tag::Heading { .. }) => {}
            Event::End(TagEnd::Heading(_)) => text.push_str("\n\n"),
            _ => {}
        }
    }

    text.trim().to_string()
}
```

**Step 4: Run tests**

Run: `cargo test -p modo-email`
Expected: PASS

**Step 5: Commit**

```bash
git add modo-email/src/template/markdown.rs
git commit -m "feat(modo-email): add Markdown renderer with button link support"
```

---

### Task 10: Layout Engine + Built-in Default Layout

**Files:**
- Modify: `modo-email/src/template/layout.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;

    #[test]
    fn render_with_builtin_layout() {
        let engine = LayoutEngine::builtin_only();
        let mut ctx = HashMap::new();
        ctx.insert("content".to_string(), serde_json::json!("<p>Hello</p>"));
        ctx.insert("subject".to_string(), serde_json::json!("Test"));

        let html = engine.render("default", &ctx).unwrap();
        assert!(html.contains("<p>Hello</p>"));
        assert!(html.contains("Test")); // subject in <title>
        assert!(html.contains("max-width")); // responsive wrapper
    }

    #[test]
    fn custom_layout_overrides_builtin() {
        let dir = tempfile::tempdir().unwrap();
        let layouts_dir = dir.path().join("layouts");
        fs::create_dir_all(&layouts_dir).unwrap();
        fs::write(
            layouts_dir.join("default.html"),
            "<html><body>CUSTOM: {{content}}</body></html>",
        ).unwrap();

        let engine = LayoutEngine::new(dir.path().to_str().unwrap());
        let mut ctx = HashMap::new();
        ctx.insert("content".to_string(), serde_json::json!("<p>Hi</p>"));

        let html = engine.render("default", &ctx).unwrap();
        assert!(html.contains("CUSTOM: <p>Hi</p>"));
    }

    #[test]
    fn missing_layout_errors() {
        let engine = LayoutEngine::builtin_only();
        let ctx = HashMap::new();
        let result = engine.render("nonexistent", &ctx);
        assert!(result.is_err());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-email`
Expected: FAIL

**Step 3: Create the built-in default layout**

Hard-coded as a const string in the module. This is the responsive HTML email wrapper:

```rust
pub(crate) const DEFAULT_LAYOUT: &str = r#"<!DOCTYPE html>
<html lang="en" xmlns:v="urn:schemas-microsoft-com:vml">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="X-UA-Compatible" content="IE=edge">
<title>{{subject}}</title>
<style>
  @media (prefers-color-scheme: dark) {
    body { background-color: #1a1a1a !important; }
    .email-wrapper { background-color: #2d2d2d !important; }
    .email-body { color: #e0e0e0 !important; }
  }
  @media only screen and (max-width: 620px) {
    .email-wrapper { width: 100% !important; padding: 16px !important; }
  }
</style>
</head>
<body style="margin:0;padding:0;background-color:#f4f4f5;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background-color:#f4f4f5">
<tr><td align="center" style="padding:32px 16px">
  <!--[if mso]><table role="presentation" width="600" cellpadding="0" cellspacing="0"><tr><td><![endif]-->
  <table role="presentation" class="email-wrapper" cellpadding="0" cellspacing="0" style="max-width:600px;width:100%;background-color:#ffffff;border-radius:8px;overflow:hidden">
    {% if logo_url %}
    <tr><td style="padding:24px 32px 0;text-align:center">
      <img src="{{logo_url}}" alt="{{product_name | default(value="")}}" style="max-height:48px;width:auto">
    </td></tr>
    {% endif %}
    <tr><td class="email-body" style="padding:32px;color:#1f2937;font-size:16px;line-height:1.6">
      {{content}}
    </td></tr>
    <tr><td style="padding:16px 32px 32px;color:#6b7280;font-size:13px;text-align:center;border-top:1px solid #e5e7eb">
      {{footer_text | default(value="")}}
    </td></tr>
  </table>
  <!--[if mso]></td></tr></table><![endif]-->
</td></tr>
</table>
</body>
</html>"#;
```

**Step 4: Implement LayoutEngine**

```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(crate) const DEFAULT_LAYOUT: &str = /* as above */;

pub struct LayoutEngine {
    env: minijinja::Environment<'static>,
}

impl LayoutEngine {
    pub fn new(templates_path: &str) -> Self {
        let mut env = minijinja::Environment::new();
        // Register built-in default
        env.add_template_owned("__builtin__/default.html".to_string(), DEFAULT_LAYOUT.to_string())
            .expect("built-in layout is valid");

        // Load custom layouts from {templates_path}/layouts/
        let layouts_dir = Path::new(templates_path).join("layouts");
        if layouts_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&layouts_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "html") {
                        if let (Some(stem), Ok(content)) = (
                            path.file_stem().and_then(|s| s.to_str()),
                            std::fs::read_to_string(&path),
                        ) {
                            env.add_template_owned(
                                format!("layouts/{stem}.html"),
                                content,
                            ).ok();
                        }
                    }
                }
            }
        }

        Self { env }
    }

    pub fn builtin_only() -> Self {
        let mut env = minijinja::Environment::new();
        env.add_template_owned("__builtin__/default.html".to_string(), DEFAULT_LAYOUT.to_string())
            .expect("built-in layout is valid");
        Self { env }
    }

    pub fn render(
        &self,
        layout_name: &str,
        context: &HashMap<String, serde_json::Value>,
    ) -> Result<String, modo::Error> {
        // Try custom layout first, then builtin
        let template_name = format!("layouts/{layout_name}.html");
        let builtin_name = format!("__builtin__/{layout_name}.html");

        let tmpl = self.env.get_template(&template_name)
            .or_else(|_| self.env.get_template(&builtin_name))
            .map_err(|_| modo::Error::internal(format!("Layout not found: {layout_name}")))?;

        let ctx = minijinja::Value::from_serialize(context);
        tmpl.render(ctx)
            .map_err(|e| modo::Error::internal(format!("Layout render error: {e}")))
    }
}
```

**Step 5: Run tests**

Run: `cargo test -p modo-email`
Expected: PASS

**Step 6: Commit**

```bash
git add modo-email/src/template/layout.rs
git commit -m "feat(modo-email): add layout engine with built-in responsive default"
```

---

### Task 11: Mailer Service + Factory Functions

**Files:**
- Modify: `modo-email/src/mailer.rs`
- Modify: `modo-email/src/lib.rs` (add factory re-exports)

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::TemplateProvider;
    use crate::transport::MailTransport;
    use std::sync::{Arc, Mutex};

    struct MockTransport {
        sent: Arc<Mutex<Vec<MailMessage>>>,
    }

    #[async_trait::async_trait]
    impl MailTransport for MockTransport {
        async fn send(&self, message: &MailMessage) -> Result<(), modo::Error> {
            self.sent.lock().unwrap().push(MailMessage {
                from: message.from.clone(),
                reply_to: message.reply_to.clone(),
                to: message.to.clone(),
                subject: message.subject.clone(),
                html: message.html.clone(),
                text: message.text.clone(),
            });
            Ok(())
        }
    }

    struct MockTemplateProvider;

    impl TemplateProvider for MockTemplateProvider {
        fn get(&self, _name: &str, _locale: &str) -> Result<crate::template::EmailTemplate, modo::Error> {
            Ok(crate::template::EmailTemplate {
                subject: "Hello {{name}}".to_string(),
                body: "Hi **{{name}}**!\n\n[button|Click](https://example.com)".to_string(),
                layout: None,
            })
        }
    }

    #[tokio::test]
    async fn send_renders_and_delivers() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let mailer = Mailer::new(
            Box::new(MockTransport { sent: sent.clone() }),
            Box::new(MockTemplateProvider),
            SenderProfile {
                from_name: "Test".to_string(),
                from_email: "test@test.com".to_string(),
                reply_to: None,
            },
            LayoutEngine::builtin_only(),
        );

        mailer.send(
            SendEmail::new("welcome", "user@test.com")
                .var("name", "Alice")
        ).await.unwrap();

        let messages = sent.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].to, "user@test.com");
        assert_eq!(messages[0].subject, "Hello Alice");
        assert!(messages[0].html.contains("Alice"));
        assert!(messages[0].html.contains("role=\"presentation\"")); // button
        assert!(messages[0].text.contains("Alice"));
    }

    #[tokio::test]
    async fn sender_override() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let mailer = Mailer::new(
            Box::new(MockTransport { sent: sent.clone() }),
            Box::new(MockTemplateProvider),
            SenderProfile {
                from_name: "Default".to_string(),
                from_email: "default@test.com".to_string(),
                reply_to: None,
            },
            LayoutEngine::builtin_only(),
        );

        let custom_sender = SenderProfile {
            from_name: "Tenant".to_string(),
            from_email: "tenant@custom.com".to_string(),
            reply_to: Some("support@custom.com".to_string()),
        };

        mailer.send(
            SendEmail::new("welcome", "user@test.com")
                .sender(&custom_sender)
                .var("name", "Bob")
        ).await.unwrap();

        let messages = sent.lock().unwrap();
        assert!(messages[0].from.contains("tenant@custom.com"));
        assert_eq!(messages[0].reply_to.as_deref(), Some("support@custom.com"));
    }

    #[test]
    fn render_returns_message_without_sending() {
        let mailer = Mailer::new(
            Box::new(MockTransport { sent: Arc::new(Mutex::new(Vec::new())) }),
            Box::new(MockTemplateProvider),
            SenderProfile {
                from_name: "Test".to_string(),
                from_email: "test@test.com".to_string(),
                reply_to: None,
            },
            LayoutEngine::builtin_only(),
        );

        let msg = mailer.render(
            &SendEmail::new("welcome", "user@test.com")
                .var("name", "Charlie")
        ).unwrap();

        assert_eq!(msg.subject, "Hello Charlie");
        assert!(msg.html.contains("Charlie"));
        assert!(msg.text.contains("Charlie"));
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-email`
Expected: FAIL

**Step 3: Implement Mailer**

```rust
use crate::config::EmailConfig;
use crate::message::{MailMessage, SendEmail, SenderProfile};
use crate::template::layout::LayoutEngine;
use crate::template::markdown;
use crate::template::vars;
use crate::template::TemplateProvider;
use crate::transport::MailTransport;
use std::collections::HashMap;

pub struct Mailer {
    transport: Box<dyn MailTransport>,
    templates: Box<dyn TemplateProvider>,
    default_sender: SenderProfile,
    layout_engine: LayoutEngine,
}

impl Mailer {
    pub fn new(
        transport: Box<dyn MailTransport>,
        templates: Box<dyn TemplateProvider>,
        default_sender: SenderProfile,
        layout_engine: LayoutEngine,
    ) -> Self {
        Self {
            transport,
            templates,
            default_sender,
            layout_engine,
        }
    }

    pub fn render(&self, email: &SendEmail) -> Result<MailMessage, modo::Error> {
        let locale = email.locale.as_deref().unwrap_or("");
        let template = self.templates.get(&email.template, locale)?;

        // Variable substitution on subject and body
        let subject = vars::substitute(&template.subject, &email.context);
        let body = vars::substitute(&template.body, &email.context);

        // Extract button color from context, fallback to default
        let button_color = email.context
            .get("brand_color")
            .and_then(|v| v.as_str())
            .unwrap_or("#4F46E5");

        // Render Markdown to HTML
        let html_body = markdown::render_markdown_with_color(&body, button_color);
        let text = markdown::render_plain_text(&body);

        // Wrap in layout
        let layout_name = template.layout.as_deref().unwrap_or("default");
        let mut layout_ctx: HashMap<String, serde_json::Value> = email.context.clone();
        layout_ctx.insert("content".to_string(), serde_json::Value::String(html_body));
        layout_ctx.insert("subject".to_string(), serde_json::Value::String(subject.clone()));

        let html = self.layout_engine.render(layout_name, &layout_ctx)?;

        // Resolve sender
        let sender = email.sender.as_ref().unwrap_or(&self.default_sender);

        Ok(MailMessage {
            from: sender.format_address(),
            reply_to: sender.reply_to.clone(),
            to: email.to.clone(),
            subject,
            html,
            text,
        })
    }

    pub async fn send(&self, email: SendEmail) -> Result<(), modo::Error> {
        let message = self.render(&email)?;
        self.transport.send(&message).await
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p modo-email`
Expected: PASS

**Step 5: Commit**

```bash
git add modo-email/src/mailer.rs
git commit -m "feat(modo-email): add Mailer service with render and send"
```

---

### Task 12: Factory Functions + Transport Factory

**Files:**
- Modify: `modo-email/src/transport/mod.rs` (add factory function)
- Modify: `modo-email/src/lib.rs` (add `mailer()` and `mailer_with()`)

**Step 1: Add transport factory**

In `transport/mod.rs`, add:

```rust
use crate::config::EmailConfig;

pub fn transport(config: &EmailConfig) -> Result<Box<dyn MailTransport>, modo::Error> {
    match config.transport {
        #[cfg(feature = "smtp")]
        crate::config::TransportBackend::Smtp => {
            Ok(Box::new(smtp::SmtpTransport::new(&config.smtp)?))
        }
        #[cfg(not(feature = "smtp"))]
        crate::config::TransportBackend::Smtp => Err(modo::Error::internal(
            "SMTP transport requires the `smtp` feature",
        )),

        #[cfg(feature = "resend")]
        crate::config::TransportBackend::Resend => {
            Ok(Box::new(resend::ResendTransport::new(&config.resend)?))
        }
        #[cfg(not(feature = "resend"))]
        crate::config::TransportBackend::Resend => Err(modo::Error::internal(
            "Resend transport requires the `resend` feature",
        )),
    }
}
```

**Step 2: Add public factory functions in lib.rs**

```rust
use crate::config::EmailConfig;
use crate::mailer::Mailer;
use crate::message::SenderProfile;
use crate::template::filesystem::FilesystemProvider;
use crate::template::layout::LayoutEngine;
use crate::template::TemplateProvider;

/// Create a Mailer with the default FilesystemProvider.
pub fn mailer(config: &EmailConfig) -> Result<Mailer, modo::Error> {
    let transport = transport::transport(config)?;
    let provider = Box::new(FilesystemProvider::new(&config.templates_path));
    let layout = LayoutEngine::new(&config.templates_path);
    let sender = SenderProfile {
        from_name: config.default_from_name.clone(),
        from_email: config.default_from_email.clone(),
        reply_to: config.default_reply_to.clone(),
    };
    Ok(Mailer::new(transport, provider, sender, layout))
}

/// Create a Mailer with a custom TemplateProvider.
pub fn mailer_with(
    config: &EmailConfig,
    provider: Box<dyn TemplateProvider>,
) -> Result<Mailer, modo::Error> {
    let transport = transport::transport(config)?;
    let layout = LayoutEngine::new(&config.templates_path);
    let sender = SenderProfile {
        from_name: config.default_from_name.clone(),
        from_email: config.default_from_email.clone(),
        reply_to: config.default_reply_to.clone(),
    };
    Ok(Mailer::new(transport, provider, sender, layout))
}
```

**Step 3: Verify it compiles**

Run: `cargo check -p modo-email`
Expected: PASS

**Step 4: Commit**

```bash
git add modo-email/src/transport/mod.rs modo-email/src/lib.rs
git commit -m "feat(modo-email): add transport factory and public mailer constructors"
```

---

### Task 13: Final Integration — Wire Up lib.rs Exports

**Files:**
- Modify: `modo-email/src/lib.rs`

**Step 1: Finalize all public re-exports**

Ensure `lib.rs` exports everything documented in the design. Add `pub use` for `FilesystemProvider`, `LayoutEngine`, `render_markdown`, `render_plain_text` if useful for advanced users.

**Step 2: Run full check**

Run: `cargo check -p modo-email && cargo check -p modo-email --features resend && cargo test -p modo-email`
Expected: All PASS

**Step 3: Run workspace lint**

Run: `just fmt && just lint`
Expected: PASS. Fix any warnings (dead code, unused imports, etc.)

**Step 4: Commit**

```bash
git add modo-email/
git commit -m "feat(modo-email): finalize public API exports"
```

---

### Task 14: End-to-End Integration Test

**Files:**
- Create: `modo-email/tests/integration.rs`

**Step 1: Write integration test with filesystem templates**

```rust
use modo_email::{EmailConfig, Mailer, SendEmail, SenderProfile};
use modo_email::template::filesystem::FilesystemProvider;
use modo_email::template::layout::LayoutEngine;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// Reuse a simple mock transport for testing
struct CapturingTransport {
    messages: Arc<Mutex<Vec<modo_email::MailMessage>>>,
}

#[async_trait::async_trait]
impl modo_email::MailTransport for CapturingTransport {
    async fn send(&self, message: &modo_email::MailMessage) -> Result<(), modo::Error> {
        self.messages.lock().unwrap().push(modo_email::MailMessage {
            from: message.from.clone(),
            reply_to: message.reply_to.clone(),
            to: message.to.clone(),
            subject: message.subject.clone(),
            html: message.html.clone(),
            text: message.text.clone(),
        });
        Ok(())
    }
}

#[tokio::test]
async fn end_to_end_filesystem_template() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    // Create template
    std::fs::write(
        path.join("welcome.md"),
        "---\nsubject: \"Welcome {{name}}!\"\n---\n\nHi **{{name}}**,\n\nGet started:\n\n[button|Launch Dashboard]({{url}})\n",
    ).unwrap();

    let messages = Arc::new(Mutex::new(Vec::new()));
    let provider = Box::new(FilesystemProvider::new(path.to_str().unwrap()));
    let layout = LayoutEngine::new(path.to_str().unwrap());

    let mailer = Mailer::new(
        Box::new(CapturingTransport { messages: messages.clone() }),
        provider,
        SenderProfile {
            from_name: "App".to_string(),
            from_email: "app@test.com".to_string(),
            reply_to: None,
        },
        layout,
    );

    mailer.send(
        SendEmail::new("welcome", "user@example.com")
            .var("name", "Alice")
            .var("url", "https://app.com/dashboard")
    ).await.unwrap();

    let msgs = messages.lock().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].subject, "Welcome Alice!");
    assert!(msgs[0].html.contains("Alice"));
    assert!(msgs[0].html.contains("Launch Dashboard"));
    assert!(msgs[0].html.contains("https://app.com/dashboard"));
    assert!(msgs[0].html.contains("role=\"presentation\"")); // button rendered
    assert!(msgs[0].text.contains("Launch Dashboard (https://app.com/dashboard)"));
}

#[tokio::test]
async fn end_to_end_locale_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();

    // Root template only
    std::fs::write(
        path.join("reset.md"),
        "---\nsubject: \"Reset password\"\n---\n\nClick below to reset.",
    ).unwrap();

    let messages = Arc::new(Mutex::new(Vec::new()));
    let mailer = Mailer::new(
        Box::new(CapturingTransport { messages: messages.clone() }),
        Box::new(FilesystemProvider::new(path.to_str().unwrap())),
        SenderProfile {
            from_name: "App".to_string(),
            from_email: "app@test.com".to_string(),
            reply_to: None,
        },
        LayoutEngine::new(path.to_str().unwrap()),
    );

    // Request "fr" locale — should fallback to root
    mailer.send(
        SendEmail::new("reset", "user@example.com").locale("fr")
    ).await.unwrap();

    let msgs = messages.lock().unwrap();
    assert_eq!(msgs[0].subject, "Reset password");
}
```

**Step 2: Run integration tests**

Run: `cargo test -p modo-email --test integration`
Expected: PASS

**Step 3: Commit**

```bash
git add modo-email/tests/
git commit -m "test(modo-email): add end-to-end integration tests"
```

---

### Task 15: README

**Files:**
- Create: `modo-email/README.md`

Write README with:
- Overview (what it does)
- Quick start (config + setup + send)
- Template format (frontmatter + Markdown + button syntax)
- Directory structure (single-language and multi-language)
- Multi-tenant usage (SenderProfile + brand context)
- Async sending via modo-jobs (app-level example)
- Custom TemplateProvider example
- Available transports and feature flags
- Configuration reference

**Step 1: Write README**

Content based on the design doc's usage examples and configuration reference.

**Step 2: Commit**

```bash
git add modo-email/README.md
git commit -m "docs(modo-email): add README with usage examples"
```

---

### Task 16: Final Verification

**Step 1: Run full workspace check**

```bash
just fmt && just check
```

Expected: All PASS

**Step 2: Review all public types match the design doc**

Verify these types are exported from `modo_email`:
- `EmailConfig`, `TransportBackend`, `SmtpConfig`, `ResendConfig`
- `Mailer`
- `SendEmail`, `SendEmailPayload`, `SenderProfile`, `MailMessage`
- `EmailTemplate`, `TemplateProvider`
- `MailTransport`
- `mailer()`, `mailer_with()`

**Step 3: Final commit if any adjustments**

```bash
git add -A
git commit -m "chore(modo-email): final cleanup"
```
