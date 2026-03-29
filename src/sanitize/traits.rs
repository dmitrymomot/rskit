/// Trait for types that can sanitize their own fields.
///
/// Implement this on request/input structs to normalize data
/// (trim whitespace, lowercase emails, etc.) before validation.
///
/// The `JsonRequest`, `FormRequest`, `Query`, and `MultipartRequest` extractors
/// call `sanitize()` automatically after deserialization, so every bound on
/// those extractors requires `T: Sanitize`.
pub trait Sanitize {
    /// Normalize the fields of `self` in place.
    ///
    /// Typical implementations call the helper functions from
    /// [`crate::sanitize`] on each `String` field.
    fn sanitize(&mut self);
}
