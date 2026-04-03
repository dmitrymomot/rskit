//! # modo::dns
//!
//! DNS-based domain ownership verification.
//!
//! Checks TXT record ownership and CNAME routing via raw UDP DNS queries.
//! Intended for custom-domain flows where a user must prove they control a
//! domain before activating it.
//!
//! Requires feature `"dns"`.
//!
//! # Provides
//!
//! - [`DnsConfig`] — nameserver address, TXT prefix, and timeout.
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
//!
//! # Feature flag
//!
//! This module is only compiled when the `dns` feature is enabled.
//!
//! ```toml
//! [dependencies]
//! modo = { version = "0.6", features = ["dns"] }
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
