mod config;
mod error;
mod protocol;
pub(crate) mod resolver;
mod token;

pub use config::DnsConfig;
pub use error::DnsError;
pub use token::generate_verification_token;
