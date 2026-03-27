//! Input validation for request data.
//!
//! This module provides a fluent [`Validator`] builder that collects per-field
//! errors and returns them all at once as a [`ValidationError`], and a
//! [`Validate`] trait for types that validate themselves.
//!
//! [`ValidationError`] converts automatically into [`crate::Error`] via the
//! `From` impl, producing an HTTP 422 Unprocessable Entity response whose
//! `details` field contains the per-field error map.
//!
//! # Example
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
