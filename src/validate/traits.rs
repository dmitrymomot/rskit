use super::ValidationError;

pub trait Validate {
    fn validate(&self) -> Result<(), ValidationError>;
}
