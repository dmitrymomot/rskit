use crate::FromMultipart;
use axum::extract::FromRequest;
use axum::http::Request;
use modo::app::AppState;
use modo::error::{Error, HttpError};
use modo::validate::Validate;
use std::ops::Deref;

/// Extractor that parses `multipart/form-data`, auto-sanitizes text fields,
/// and provides manual `.validate()`.
pub struct MultipartForm<T>(pub T);

impl<T> Deref for MultipartForm<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> MultipartForm<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Validate> MultipartForm<T> {
    pub fn validate(&self) -> Result<(), Error> {
        self.0.validate()
    }
}

impl<T> FromRequest<AppState> for MultipartForm<T>
where
    T: FromMultipart + 'static,
{
    type Rejection = Error;

    async fn from_request(
        req: Request<axum::body::Body>,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let mut multipart = axum::extract::Multipart::from_request(req, state)
            .await
            .map_err(|e| HttpError::BadRequest.with_message(format!("{e}")))?;
        let default_config = crate::config::UploadConfig::default();
        let registered_config = state.services.get::<crate::config::UploadConfig>();
        let config = registered_config.as_deref().unwrap_or(&default_config);
        let max_file_size = config.max_file_size.as_ref().and_then(|s| {
            modo::config::parse_size(s)
                .inspect_err(|e| {
                    modo::tracing::warn!(
                        size = %s,
                        error = %e,
                        "failed to parse max_file_size from UploadConfig, ignoring limit"
                    );
                })
                .ok()
        });
        let mut value = T::from_multipart(&mut multipart, max_file_size).await?;
        modo::sanitize::auto_sanitize(&mut value);
        Ok(MultipartForm(value))
    }
}
