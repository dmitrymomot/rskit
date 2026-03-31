use super::ValidationError;

/// Types that can validate their own fields.
///
/// Implement this trait on request structs and call `validate()` inside your
/// handler before processing the input. Use `?` to propagate the
/// [`ValidationError`] as a [`crate::Error`] (HTTP 422).
///
/// # Example
///
/// ```rust,no_run
/// use modo::validate::{Validate, ValidationError, Validator};
///
/// struct SignUp {
///     username: String,
/// }
///
/// impl Validate for SignUp {
///     fn validate(&self) -> Result<(), ValidationError> {
///         Validator::new()
///             .field("username", &self.username, |f| f.required().min_length(3))
///             .check()
///     }
/// }
/// ```
pub trait Validate {
    /// Validate this value, returning all field-level errors at once.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] containing every failing field and its
    /// error messages when one or more fields are invalid.
    fn validate(&self) -> Result<(), ValidationError>;
}
