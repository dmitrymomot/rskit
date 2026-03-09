#[cfg(feature = "resend")]
pub mod resend;
#[cfg(feature = "smtp")]
pub mod smtp;

use crate::config::EmailConfig;
use crate::message::MailMessage;

#[async_trait::async_trait]
pub trait MailTransport: Send + Sync + 'static {
    async fn send(&self, message: &MailMessage) -> Result<(), modo::Error>;
}

/// Create the appropriate transport backend based on config.
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
