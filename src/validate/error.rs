use std::collections::HashMap;
use std::fmt;

pub struct ValidationError {
    fields: HashMap<String, Vec<String>>,
}

impl ValidationError {
    pub fn new(fields: HashMap<String, Vec<String>>) -> Self {
        Self { fields }
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn field_errors(&self, field: &str) -> &[String] {
        self.fields.get(field).map(|v| v.as_slice()).unwrap_or(&[])
    }

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
