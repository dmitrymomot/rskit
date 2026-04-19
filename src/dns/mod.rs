//! # modo::dns
//!
//! DNS-based domain ownership verification via raw UDP TXT and CNAME lookups.
//!
//! Intended for custom-domain flows where a user must prove they control a
//! domain before activating it. Queries are sent directly to a configured
//! nameserver, bypassing the system resolver.
//!
//! Provides:
//! - [`DnsConfig`] — nameserver address, TXT prefix, and timeout
//!   (deserializes from YAML).
//! - [`DomainVerifier`] — performs TXT and CNAME lookups; `Arc`-backed, cheap
//!   to clone.
//! - [`DomainStatus`] — result of [`DomainVerifier::verify_domain`] with
//!   individual `txt_verified` / `cname_verified` booleans.
//! - [`DnsError`] — error variants with stable `"dns:<kind>"` codes.
//! - [`generate_verification_token`] — 13-char base36 token for TXT challenges.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use modo::dns::{DnsConfig, DomainVerifier, generate_verification_token};
//!
//! # async fn example() -> modo::Result<()> {
//! let config = DnsConfig::new("8.8.8.8:53");
//! let verifier = DomainVerifier::from_config(&config)?;
//! let token = generate_verification_token();
//!
//! // User creates TXT record: _modo-verify.example.com -> "<token>"
//! let txt_ok = verifier.check_txt("example.com", &token).await?;
//! # Ok(())
//! # }
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
