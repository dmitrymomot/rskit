use super::MailTransportSend;
use crate::config::SmtpConfig;
use crate::message::MailMessage;
use lettre::message::{MultiPart, SinglePart, header::ContentType};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

/// SMTP delivery backend backed by [lettre](https://docs.rs/lettre).
///
/// Requires the `smtp` feature.
pub struct SmtpTransport {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpTransport {
    /// Create an SMTP transport from `SmtpConfig`.
    ///
    /// - `SmtpSecurity::None` — plaintext, no TLS (local dev / trusted relay).
    /// - `SmtpSecurity::StartTls` — upgrades a plaintext connection via STARTTLS (port 587).
    /// - `SmtpSecurity::ImplicitTls` — direct TLS connection, SMTPS (port 465).
    pub fn new(config: &SmtpConfig) -> Result<Self, modo::Error> {
        let builder = match config.security {
            crate::config::SmtpSecurity::None => {
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
                    .port(config.port)
            }
            crate::config::SmtpSecurity::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
                    .map_err(|e| modo::Error::internal(format!("SMTP config error: {e}")))?
                    .port(config.port)
            }
            crate::config::SmtpSecurity::ImplicitTls => {
                AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
                    .map_err(|e| modo::Error::internal(format!("SMTP config error: {e}")))?
                    .port(config.port)
            }
        };

        let mailer = if !config.username.is_empty() {
            let creds = Credentials::new(config.username.clone(), config.password.clone());
            builder.credentials(creds).build()
        } else {
            builder.build()
        };

        Ok(Self { mailer })
    }
}

impl MailTransportSend for SmtpTransport {
    async fn send(&self, message: &MailMessage) -> Result<(), modo::Error> {
        let mut builder = Message::builder()
            .from(
                message
                    .from
                    .parse()
                    .map_err(|e| modo::Error::internal(format!("invalid from address: {e}")))?,
            )
            .subject(&message.subject);

        for recipient in &message.to {
            builder = builder.to(recipient
                .parse()
                .map_err(|e| modo::Error::internal(format!("invalid to address: {e}")))?);
        }

        if let Some(ref reply_to) = message.reply_to {
            builder =
                builder.reply_to(reply_to.parse().map_err(|e| {
                    modo::Error::internal(format!("invalid reply-to address: {e}"))
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
            .map_err(|e| modo::Error::internal(format!("failed to build email: {e}")))?;

        tracing::debug!(to = ?message.to, subject = %message.subject, "sending email via SMTP");

        self.mailer.send(email).await.map_err(|e| {
            tracing::error!(error = %e, "SMTP send failed");
            modo::Error::internal(format!("SMTP send failed: {e}"))
        })?;

        Ok(())
    }
}
