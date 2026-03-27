use std::ops::Deref;

/// A typed wrapper around a deserialized job payload.
///
/// Use `Payload<T>` as a parameter in job handler functions to receive the
/// deserialized payload. `T` must implement [`serde::de::DeserializeOwned`].
///
/// `Payload<T>` implements [`Deref<Target = T>`], so methods on `T` are
/// accessible directly without `.0`.
///
/// # Example
///
/// ```rust,no_run
/// use modo::job::Payload;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct WelcomePayload { user_id: String }
///
/// async fn handle(payload: Payload<WelcomePayload>) -> modo::Result<()> {
///     println!("user_id = {}", payload.user_id);
///     Ok(())
/// }
/// ```
pub struct Payload<T>(pub T);

impl<T> Deref for Payload<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
