//! Unique ID generation utilities.
//!
//! Provides two functions for generating time-sortable unique identifiers:
//!
//! - [`ulid()`] — 26-character spec-compliant ULID (Crockford base32, uppercase),
//!   suitable for primary keys and globally unique identifiers.
//! - [`short()`] — 13-character base36 ID (lowercase), suitable for user-visible
//!   codes, slugs, and short URLs.
//!
//! Both functions are always available and require no feature flags.
mod short;
mod ulid;

pub use short::short;
pub use ulid::ulid;
