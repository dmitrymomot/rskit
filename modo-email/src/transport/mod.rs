#[cfg(feature = "smtp")]
pub mod smtp;
#[cfg(feature = "resend")]
pub mod resend;

use crate::message::MailMessage;

#[async_trait::async_trait]
pub trait MailTransport: Send + Sync + 'static {
    async fn send(&self, message: &MailMessage) -> Result<(), modo::Error>;
}
