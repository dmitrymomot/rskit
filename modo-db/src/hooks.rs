/// Lifecycle hooks for database entities.
///
/// Provides no-op default implementations for `before_save`, `after_save`, and
/// `before_delete` via a blanket impl on all types.
///
/// When the `#[entity]` macro generates `insert`/`update`/`delete` methods it
/// calls `self.before_save()` etc. using this trait.  If the user defines an
/// inherent method with the same name and signature on their struct, Rust's
/// inherent-method priority means the user's method takes precedence.  If no
/// inherent method is defined the blanket default (no-op) fires automatically.
pub trait DefaultHooks {
    /// Called before the entity is inserted or updated.
    fn before_save(&mut self) -> Result<(), modo::Error> {
        Ok(())
    }

    /// Called after the entity has been successfully inserted or updated.
    fn after_save(&self) -> Result<(), modo::Error> {
        Ok(())
    }

    /// Called before the entity is deleted.
    fn before_delete(&self) -> Result<(), modo::Error> {
        Ok(())
    }
}

impl<T> DefaultHooks for T {}
