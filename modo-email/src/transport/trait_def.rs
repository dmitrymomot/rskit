use crate::message::MailMessage;
use std::future::Future;
use std::pin::Pin;

/// Async trait that every delivery backend must implement.
///
/// Implement this trait to add a custom transport (e.g. a test spy,
/// an in-memory queue, or a third-party HTTP API).
///
/// Use [`MailTransportDyn`] (object-safe companion) for trait objects:
/// `Arc<dyn MailTransportDyn>`. Any type implementing `MailTransport`
/// automatically implements `MailTransportDyn` via a blanket impl.
#[trait_variant::make(MailTransportSend: Send)]
pub trait MailTransport: Sync + 'static {
    /// Deliver `message` to its recipients.
    async fn send(&self, message: &MailMessage) -> Result<(), modo::Error>;
}

/// Object-safe companion to [`MailTransport`] for use with `Arc<dyn MailTransportDyn>`.
///
/// This trait is automatically implemented for all types that implement
/// [`MailTransport`] (or `MailTransportSend`).
pub trait MailTransportDyn: Send + Sync + 'static {
    /// Deliver `message` to its recipients.
    fn send<'a>(
        &'a self,
        message: &'a MailMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), modo::Error>> + Send + 'a>>;
}

impl<T: MailTransportSend> MailTransportDyn for T {
    fn send<'a>(
        &'a self,
        message: &'a MailMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), modo::Error>> + Send + 'a>> {
        Box::pin(MailTransportSend::send(self, message))
    }
}
