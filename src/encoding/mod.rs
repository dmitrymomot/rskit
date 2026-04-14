//! # modo::encoding
//!
//! Binary-to-text encoding utilities.
//!
//! Provides three submodules:
//!
//! | Submodule     | Standard | Alphabet         | Padding | Extra            |
//! | ------------- | -------- | ---------------- | ------- | ---------------- |
//! | [`base32`]    | RFC 4648 | `A–Z`, `2–7`     | none    | —                |
//! | [`base64url`] | RFC 4648 | `A–Za–z0–9-_`    | none    | —                |
//! | [`hex`]       | —        | `0–9`, `a–f`     | —       | [`hex::sha256`]  |
//!
//! The [`base32`] and [`base64url`] submodules each expose an `encode` / `decode`
//! pair. The [`hex`] submodule exposes `encode` (no `decode`) plus a convenience
//! [`hex::sha256`] function that returns a 64-character hex digest.
//!
//! # Examples
//!
//! ```rust
//! use modo::encoding::{base32, base64url, hex};
//!
//! let b32 = base32::encode(b"foobar");
//! assert_eq!(b32, "MZXW6YTBOI");
//!
//! let b64 = base64url::encode(b"Hello");
//! assert_eq!(b64, "SGVsbG8");
//!
//! let h = hex::encode(b"\xde\xad");
//! assert_eq!(h, "dead");
//!
//! let digest = hex::sha256(b"hello world");
//! assert_eq!(digest.len(), 64);
//! ```

pub mod base32;
pub mod base64url;
pub mod hex;
