mod config;
mod mailer;
mod message;
pub mod template;
pub mod transport;

pub use config::{EmailConfig, TransportBackend};
pub use mailer::Mailer;
pub use message::{MailMessage, SendEmail, SendEmailPayload, SenderProfile};
pub use template::{EmailTemplate, TemplateProvider};
pub use transport::MailTransport;

#[cfg(feature = "resend")]
pub use config::ResendConfig;
#[cfg(feature = "smtp")]
pub use config::SmtpConfig;

pub use template::filesystem::FilesystemProvider;
pub use template::layout::LayoutEngine;

use std::sync::Arc;

/// Create a Mailer with the default FilesystemProvider.
pub fn mailer(config: &EmailConfig) -> Result<Mailer, modo::Error> {
    let provider = Arc::new(FilesystemProvider::new(&config.templates_path));
    mailer_with(config, provider)
}

/// Create a Mailer with a custom TemplateProvider.
pub fn mailer_with(
    config: &EmailConfig,
    provider: Arc<dyn TemplateProvider>,
) -> Result<Mailer, modo::Error> {
    let transport = transport::transport(config)?;
    let layout = Arc::new(LayoutEngine::new(&config.templates_path));
    let sender = SenderProfile {
        from_name: config.default_from_name.clone(),
        from_email: config.default_from_email.clone(),
        reply_to: config.default_reply_to.clone(),
    };
    Ok(Mailer::new(transport, provider, sender, layout))
}
