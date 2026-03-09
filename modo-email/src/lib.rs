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

use template::filesystem::FilesystemProvider;
use template::layout::LayoutEngine;

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
