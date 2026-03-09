use super::MailTransport;
use crate::config::SmtpConfig;
use crate::message::MailMessage;
use lettre::message::{MultiPart, SinglePart, header::ContentType};
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
                .map_err(|e| modo::Error::internal(format!("SMTP config error: {e}")))?
                .port(config.port)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host).port(config.port)
        };

        let mailer = builder.credentials(creds).build();

        Ok(Self { mailer })
    }
}

#[async_trait::async_trait]
impl MailTransport for SmtpTransport {
    async fn send(&self, message: &MailMessage) -> Result<(), modo::Error> {
        let mut builder = Message::builder()
            .from(
                message
                    .from
                    .parse()
                    .map_err(|e| modo::Error::internal(format!("Invalid from address: {e}")))?,
            )
            .to(message
                .to
                .parse()
                .map_err(|e| modo::Error::internal(format!("Invalid to address: {e}")))?)
            .subject(&message.subject);

        if let Some(ref reply_to) = message.reply_to {
            builder =
                builder.reply_to(reply_to.parse().map_err(|e| {
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
