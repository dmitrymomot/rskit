//! Email delivery with Markdown templates and SMTP transport.
//!
//! Requires feature `"email"`.
//!
//! Templates are Markdown files with a YAML frontmatter block that specifies
//! the subject line and optional layout. Variable substitution uses
//! `{{var_name}}` placeholders throughout both frontmatter and body.
//!
//! # Template format
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
