use std::collections::HashMap;
use std::fmt;

/// A collection of per-field validation errors.
///
/// Produced by [`super::Validator::check`] when one or more fields fail
/// validation. Each field maps to a list of human-readable error messages.
///
/// Converts automatically into [`crate::Error`] (HTTP 422 Unprocessable Entity)
/// via the `From` impl, with the field map serialized into the response
/// `details` field.
pub struct ValidationError {
    fields: HashMap<String, Vec<String>>,
}

impl ValidationError {
    /// Create a new `ValidationError` from a pre-built field-error map.
    ///
    /// Most callers should use [`super::Validator`] instead of constructing
    /// this directly.
    pub fn new(fields: HashMap<String, Vec<String>>) -> Self {
        Self { fields }
    }

    /// Returns `true` if no field errors have been recorded.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Returns the error messages for a single field, or an empty slice if
    /// the field had no errors.
    pub fn field_errors(&self, field: &str) -> &[String] {
        self.fields.get(field).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Returns the full map of field names to their error message lists.
    pub fn fields(&self) -> &HashMap<String, Vec<String>> {
        &self.fields
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "validation failed: {} field(s) invalid",
            self.fields.len()
        )
    }
}

impl fmt::Debug for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidationError")
            .field("fields", &self.fields)
            .finish()
    }
}

impl std::error::Error for ValidationError {}

impl From<ValidationError> for crate::error::Error {
    fn from(ve: ValidationError) -> Self {
        crate::error::Error::unprocessable_entity("validation failed")
            .with_details(serde_json::json!(ve.fields))
    }
}
