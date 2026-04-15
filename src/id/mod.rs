//! # modo::id
//!
//! Unique ID generation utilities.
//!
//! Provides:
//!
//! - [`ulid`] — 26-character spec-compliant ULID (Crockford base32, uppercase),
//!   suitable for primary keys and globally unique identifiers.
//! - [`short`] — 13-character base36 ID (lowercase), suitable for user-visible
//!   codes, slugs, and short URLs.
//!
//! Both functions are always available and require no feature flags.
//!
//! ## Quick start
//!
//! ```rust
//! use modo::id::{ulid, short};
//!
//! let pk = ulid();        // e.g. "01H5KEBZXQJ3A1BCDTG9V0KWRP"
//! let code = short();     // e.g. "0j3k7m9q2x1nz"
//! ```
mod short;
mod ulid;

pub use short::short;
pub use ulid::ulid;
