use crate::app::AppState;
use crate::error::{Error, HttpError};
use axum::extract::FromRequest;
use axum::http::{Request, StatusCode};
use std::ops::Deref;

/// Trait implemented by `#[derive(modo::Validate)]` to validate struct fields.
pub trait Validate {
    fn validate(&self) -> Result<(), Error>;
}

/// Build a validation `Error` from a list of `(field_name, messages)` pairs.
/// Only includes fields that have at least one message.
pub fn validation_error(field_errors: Vec<(&str, Vec<String>)>) -> Error {
    let mut err = Error::new(
        StatusCode::BAD_REQUEST,
        "validation_error",
        "Validation failed",
    );
    for (field, messages) in field_errors {
        if !messages.is_empty() {
            err = err.detail(field, serde_json::json!(messages));
        }
    }
    err
}

/// Simple email validation: requires text before `@`, text after `@`, and a `.` after `@`.
pub fn is_valid_email(s: &str) -> bool {
    match s.find('@') {
        Some(at) => at > 0 && at < s.len() - 1 && s[at + 1..].contains('.'),
        None => false,
    }
}

/// Extractor that deserializes `application/x-www-form-urlencoded`, auto-sanitizes,
/// and provides manual `.validate()`.
/// Works with just `T: DeserializeOwned`. If `#[derive(Sanitize)]` is present,
/// sanitization happens automatically. If `#[derive(Validate)]` is present,
/// `.validate()` becomes available.
pub struct FormReq<T>(pub T);

impl<T> Deref for FormReq<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> FormReq<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Validate> FormReq<T> {
    pub fn validate(&self) -> Result<(), Error> {
        self.0.validate()
    }
}

impl<T> FromRequest<AppState> for FormReq<T>
where
    T: serde::de::DeserializeOwned + 'static,
{
    type Rejection = Error;

    async fn from_request(
        req: Request<axum::body::Body>,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let axum::Form(mut value) = axum::Form::<T>::from_request(req, state)
            .await
            .map_err(|e| HttpError::BadRequest.with_message(format!("{e}")))?;
        crate::sanitize::auto_sanitize(&mut value);
        Ok(FormReq(value))
    }
}

/// Extractor that deserializes `application/json`, auto-sanitizes,
/// and provides manual `.validate()`.
/// Works with just `T: DeserializeOwned`. If `#[derive(Sanitize)]` is present,
/// sanitization happens automatically. If `#[derive(Validate)]` is present,
/// `.validate()` becomes available.
pub struct JsonReq<T>(pub T);

impl<T> Deref for JsonReq<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> JsonReq<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Validate> JsonReq<T> {
    pub fn validate(&self) -> Result<(), Error> {
        self.0.validate()
    }
}

impl<T> FromRequest<AppState> for JsonReq<T>
where
    T: serde::de::DeserializeOwned + 'static,
{
    type Rejection = Error;

    async fn from_request(
        req: Request<axum::body::Body>,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let axum::Json(mut value) = axum::Json::<T>::from_request(req, state)
            .await
            .map_err(|e| HttpError::BadRequest.with_message(format!("{e}")))?;
        crate::sanitize::auto_sanitize(&mut value);
        Ok(JsonReq(value))
    }
}
