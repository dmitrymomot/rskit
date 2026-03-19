/// Trait for types that can sanitize their own fields.
///
/// Implement this on request/input structs to normalize data
/// (trim whitespace, lowercase emails, etc.) before validation.
pub trait Sanitize {
    fn sanitize(&mut self);
}
