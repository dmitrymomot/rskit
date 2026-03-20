use std::ops::Deref;

pub struct Payload<T>(pub T);

impl<T> Deref for Payload<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
