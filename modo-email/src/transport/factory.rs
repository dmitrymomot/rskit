use crate::config::EmailConfig;
use crate::transport::MailTransport;
use std::sync::Arc;

/// Create the appropriate transport backend based on config.
pub fn transport(config: &EmailConfig) -> Result<Arc<dyn MailTransport>, modo::Error> {
    match config.transport {
        #[cfg(feature = "smtp")]
        crate::config::TransportBackend::Smtp => {
            Ok(Arc::new(super::smtp::SmtpTransport::new(&config.smtp)?))
        }
        #[cfg(not(feature = "smtp"))]
        crate::config::TransportBackend::Smtp => Err(modo::Error::internal(
            "SMTP transport requires the `smtp` feature",
        )),

        #[cfg(feature = "resend")]
        crate::config::TransportBackend::Resend => Ok(Arc::new(
            super::resend::ResendTransport::new(&config.resend)?,
        )),
        #[cfg(not(feature = "resend"))]
        crate::config::TransportBackend::Resend => Err(modo::Error::internal(
            "Resend transport requires the `resend` feature",
        )),
    }
}
