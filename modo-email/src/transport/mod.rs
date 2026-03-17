//! Email delivery transports.
//!
//! The [`MailTransport`] / [`MailTransportSend`] traits define the delivery contract.
//! [`MailTransportDyn`] is the object-safe form used with `Arc<dyn MailTransportDyn>`.
//!
//! Built-in backends:
//! - `smtp::SmtpTransport` — SMTP via `lettre` (requires the `smtp` feature)
//! - `resend::ResendTransport` — Resend HTTP API (requires the `resend` feature)

mod factory;
#[cfg(feature = "resend")]
pub mod resend;
#[cfg(feature = "smtp")]
pub mod smtp;
mod trait_def;

pub use factory::transport;
pub use trait_def::{MailTransport, MailTransportDyn, MailTransportSend};
