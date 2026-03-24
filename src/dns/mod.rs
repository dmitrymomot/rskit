mod config;
mod error;
mod protocol;
pub(crate) mod resolver;
mod token;
mod verifier;

pub use config::DnsConfig;
pub use error::DnsError;
pub use token::generate_verification_token;
pub use verifier::{DomainStatus, DomainVerifier};
