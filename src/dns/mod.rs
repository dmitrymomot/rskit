//! DNS-based domain verification.
//!
//! Provides [`DomainVerifier`] for checking TXT record ownership and CNAME
//! routing via raw UDP DNS queries. Intended for custom-domain flows where a
//! user must prove they control a domain before activating it.
//!
//! # Feature flag
//!
//! This module is only compiled when the `dns` feature is enabled.
//!
//! ```toml
//! [dependencies]
//! modo = { version = "*", features = ["dns"] }
//! ```

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
