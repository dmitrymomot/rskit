//! # modo::validate
//!
//! Input validation for request data.
//!
//! Provides:
//! - [`Validator`] — fluent builder that collects per-field validation errors
//! - [`ValidationError`] — per-field error collection; converts into [`crate::Error`] (HTTP 422)
//! - [`Validate`] — trait for types that validate themselves
//!
//! Field rules are applied through a `FieldValidator` obtained inside the
//! [`Validator::field`] closure. String rules require `T: AsRef<str>`; numeric
//! rules require `T: PartialOrd + Display`.
//!
//! [`ValidationError`] converts automatically into [`crate::Error`] via the
//! `From` impl, producing an HTTP 422 Unprocessable Entity response whose
//! `details` field contains the per-field error map.
//!
//! All three public items ([`Validate`], [`ValidationError`], [`Validator`])
//! are re-exported from [`crate::prelude`].
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use modo::validate::{Validate, ValidationError, Validator};
//!
//! struct CreateUser {
//!     name: String,
//!     email: String,
//!     age: u32,
//! }
//!
//! impl Validate for CreateUser {
//!     fn validate(&self) -> Result<(), ValidationError> {
//!         Validator::new()
//!             .field("name", &self.name, |f| {
//!                 f.required().min_length(2).max_length(100)
//!             })
//!             .field("email", &self.email, |f| f.required().email())
//!             .field("age", &self.age, |f| f.range(18..=120))
//!             .check()
//!     }
//! }
//! ```

mod error;
mod rules;
mod traits;
mod validator;

pub use error::ValidationError;
pub use traits::Validate;
pub use validator::Validator;
