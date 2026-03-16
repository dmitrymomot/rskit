//! Transactional email for the modo framework.
//!
//! `modo-email` provides Markdown-based email templates, responsive HTML rendering,
//! plain-text fallback generation, and pluggable delivery transports (SMTP and Resend).
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use modo_email::{mailer, EmailConfig, SendEmail};
//!
//! # async fn example() -> Result<(), modo::Error> {
//! let config = EmailConfig::default(); // load from YAML in practice
//! let m = mailer(&config)?;
//!
//! m.send(
//!     &SendEmail::new("welcome", "user@example.com")
//!         .var("name", "Alice"),
//! ).await?;
//! # Ok(())
//! # }
//! ```

mod config;
mod factory;
mod mailer;
mod message;
pub mod template;
pub mod transport;

pub use config::{EmailConfig, TransportBackend};
pub use mailer::Mailer;
pub use message::{MailMessage, SendEmail, SendEmailPayload, SenderProfile};
pub use template::{EmailTemplate, TemplateProvider};
pub use transport::{MailTransport, MailTransportDyn, MailTransportSend};

#[cfg(feature = "resend")]
pub use config::ResendConfig;
#[cfg(feature = "smtp")]
pub use config::SmtpConfig;

pub use factory::{mailer, mailer_with};
pub use template::filesystem::FilesystemProvider;
pub use template::layout::LayoutEngine;
