use crate::message::MailMessage;

/// Async trait that every delivery backend must implement.
///
/// Implement this trait to add a custom transport (e.g. a test spy,
/// an in-memory queue, or a third-party HTTP API).
#[async_trait::async_trait]
pub trait MailTransport: Send + Sync + 'static {
    /// Deliver `message` to its recipients.
    async fn send(&self, message: &MailMessage) -> Result<(), modo::Error>;
}
