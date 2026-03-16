mod factory;
#[cfg(feature = "resend")]
pub mod resend;
#[cfg(feature = "smtp")]
pub mod smtp;
mod trait_def;

pub use factory::transport;
pub use trait_def::{MailTransport, MailTransportDyn, MailTransportSend};
