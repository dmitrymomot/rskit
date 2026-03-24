use std::collections::HashMap;

use super::error::ValidationError;
use super::rules::FieldValidator;

/// A builder that collects validation errors across multiple fields.
///
/// Call [`Validator::new`] (or [`Default::default`]) to start, chain
/// [`field`](Validator::field) calls for each input to validate, then call
/// [`check`](Validator::check) to obtain the result. Errors from all fields
/// are gathered before returning — no short-circuit.
///
/// # Example
///
/// ```rust,no_run
/// use modo::validate::Validator;
///
/// let result = Validator::new()
///     .field("name", &"Alice".to_string(), |f| f.required().min_length(1))
///     .field("age", &25i32, |f| f.range(18..=120))
///     .check();
/// ```
pub struct Validator {
    errors: HashMap<String, Vec<String>>,
}

impl Validator {
    /// Create a new empty validator.
    pub fn new() -> Self {
        Self {
            errors: HashMap::new(),
        }
    }

    /// Validate a single field. The closure receives a `FieldValidator` and should
    /// chain rule methods on it. Any rule failures are collected as errors for this field.
    ///
    /// Works with any value type — string rules are available for `T: AsRef<str>`,
    /// numeric rules for `T: PartialOrd + Display`.
    pub fn field<T>(
        mut self,
        name: &str,
        value: &T,
        f: impl FnOnce(FieldValidator<'_, T>) -> FieldValidator<'_, T>,
    ) -> Self {
        let mut field_errors = Vec::new();
        let fv = FieldValidator::new(value, &mut field_errors);
        f(fv);
        if !field_errors.is_empty() {
            self.errors
                .entry(name.to_string())
                .or_default()
                .extend(field_errors);
        }
        self
    }

    /// Finalize validation. Returns `Ok(())` if no errors were collected,
    /// or `Err(ValidationError)` with all field-level errors.
    pub fn check(self) -> Result<(), ValidationError> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::new(self.errors))
        }
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}
