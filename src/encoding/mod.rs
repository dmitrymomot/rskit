//! Binary-to-text encoding utilities.
//!
//! Provides two submodules, each with a pair of free functions:
//!
//! | Submodule   | Standard | Alphabet        | Padding |
//! | ----------- | -------- | --------------- | ------- |
//! | [`base32`]  | RFC 4648 | `A–Z`, `2–7`    | none    |
//! | [`base64url`] | RFC 4648 | `A–Za–z0–9-_` | none    |
//!
//! This module is always available and requires no feature flag.
//!
//! # Examples
//!
//! ```rust
//! use modo::encoding::{base32, base64url};
//!
//! let b32 = base32::encode(b"foobar");
//! assert_eq!(b32, "MZXW6YTBOI");
//!
//! let b64 = base64url::encode(b"Hello");
//! assert_eq!(b64, "SGVsbG8");
//! ```

pub mod base32;
pub mod base64url;
