//! Email delivery with Markdown templates and SMTP transport.
//!
//! Requires feature `"email"`.
//!
//! Templates are Markdown files with a YAML frontmatter block that specifies
//! the subject line and optional layout. Variable substitution uses
//! `{{var_name}}` placeholders throughout both frontmatter and body.
//!
//! # Provides
//!
//! - [`Mailer`] — renders templates and delivers email over SMTP.
//! - [`EmailConfig`] / [`SmtpConfig`] / [`SmtpSecurity`] — configuration
//!   (deserializes from YAML).
//! - [`SendEmail`] — builder for composing an outgoing email.
//! - [`SenderProfile`] — per-message `From` / `Reply-To` override.
//! - [`RenderedEmail`] — output of [`Mailer::render`] (subject, HTML, text).
//! - [`TemplateSource`] — trait for pluggable template loaders.
//! - [`FileSource`] / [`CachedSource`] — filesystem loader and LRU-caching
//!   wrapper.
//! - [`ButtonType`] — button colour variants for email templates.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use modo::email::{EmailConfig, Mailer, SendEmail};
//!
//! # async fn run() -> modo::Result<()> {
//! let mut config = EmailConfig::default();
//! config.templates_path = "emails".into();
//! config.default_from_email = "noreply@example.com".into();
//! let mailer = Mailer::new(&config)?;
//!
//! mailer.send(
//!     SendEmail::new("welcome", "user@example.com")
//!         .var("name", "Dmytro"),
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Template format
//!
//! ```text
//! ---
//! subject: Welcome to {{app_name}}!
//! layout: base
//! ---
//!
//! Hi {{name}},
//!
//! [button|Get started](https://example.com/start)
//! ```

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
