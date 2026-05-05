//! Core [`Error`] type and [`Result`] alias.

use axum::response::{IntoResponse, Response};
use http::StatusCode;
use std::fmt;

/// A type alias for `std::result::Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// The primary error type for the modo framework.
///
/// `Error` carries:
/// - an HTTP [`StatusCode`] that will be used as the response status,
/// - a human-readable `message` string,
/// - an optional structured `details` payload (arbitrary JSON),
/// - an optional boxed `source` error for causal chaining,
/// - an optional static `error_code` string that survives the response pipeline,
/// - an optional static `locale_key` that lets the default error handler translate
///   the message at response-build time,
/// - a `lagged` flag used by the SSE broadcaster to signal that a subscriber dropped messages.
///
/// # Conversion to HTTP response
///
/// Calling `into_response()` produces a JSON body:
///
/// ```json
/// { "error": { "status": 404, "message": "user not found" } }
/// ```
///
/// If [`with_details`](Error::with_details) was called, a `"details"` field is included.
/// A copy of the error (without `source`) is also stored in response extensions so middleware
/// can inspect it after the fact.
///
/// # Clone behaviour
///
/// Cloning an `Error` drops the `source` field because `Box<dyn Error>` is not `Clone`.
/// The `error_code`, `locale_key`, `details`, and all other fields are preserved.
pub struct Error {
    status: StatusCode,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    error_code: Option<&'static str>,
    locale_key: Option<&'static str>,
    details: Option<serde_json::Value>,
    lagged: bool,
}

impl Error {
    /// Create a new error with the given HTTP status code and message.
    ///
    /// Prefer one of the named status-code constructors
    /// ([`Error::not_found`], [`Error::bad_request`], [`Error::internal`], …)
    /// when they match. Use `new` only for statuses without a dedicated
    /// constructor.
    ///
    /// # Example
    ///
    /// ```rust
    /// use modo::error::Error;
    /// use modo::axum::http::StatusCode;
    ///
    /// let err = Error::new(StatusCode::IM_A_TEAPOT, "no coffee here");
    /// assert_eq!(err.status(), StatusCode::IM_A_TEAPOT);
    /// ```
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            source: None,
            error_code: None,
            locale_key: None,
            details: None,
            lagged: false,
        }
    }

    /// Create a new error with a status code, message, and a boxed source error.
    ///
    /// `with_source` is a **constructor**, not a builder method — it wraps an
    /// underlying error at construction time. When you already have an
    /// [`Error`] and want to attach a cause, use the [`chain`](Error::chain)
    /// builder instead.
    ///
    /// # Example
    ///
    /// ```rust
    /// use modo::error::Error;
    /// use modo::axum::http::StatusCode;
    /// use std::io;
    ///
    /// let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
    /// let err = Error::with_source(StatusCode::INTERNAL_SERVER_ERROR, "read failed", io_err);
    /// assert!(err.source_as::<io::Error>().is_some());
    /// ```
    pub fn with_source(
        status: StatusCode,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            status,
            message: message.into(),
            source: Some(Box::new(source)),
            error_code: None,
            locale_key: None,
            details: None,
            lagged: false,
        }
    }

    /// Create an error whose message is a translation key.
    ///
    /// The `key` is stored in the `locale_key` slot and is also used as the raw
    /// `message`. When the
    /// [`default_error_handler`](crate::middleware::default_error_handler) runs
    /// and a [`Translator`](crate::i18n::Translator) is present in the request
    /// extensions (installed by
    /// [`I18nLayer`](crate::i18n::I18nLayer)), it resolves `key` into the
    /// user-facing string at response-build time. Without that middleware (or
    /// without a `Translator`), the response falls back to the raw key — making
    /// the behaviour predictable and easy to spot in logs.
    ///
    /// This constructor leaves `error_code`, `details`, and `source` unset;
    /// chain [`with_code`](Error::with_code),
    /// [`with_details`](Error::with_details), or [`chain`](Error::chain)
    /// afterwards if needed.
    ///
    /// # Kwargs and logging
    ///
    /// Translation kwargs (`{count}`, `{name}`, etc.) are not yet supported at
    /// the `Error` level — the default handler calls `tr.t(key, &[])` with no
    /// arguments. When you need interpolation, attach a descriptive fallback
    /// message via [`Error::with_locale_key`] and run translation (with kwargs)
    /// inside a custom handler passed to
    /// [`error_handler`](crate::middleware::error_handler).
    ///
    /// Also note that [`Debug`] and [`Display`](std::fmt::Display) print the raw key (because the
    /// fallback message _is_ the key), which makes structured logs look like
    /// `errors.user.not_found` rather than human text. Prefer
    /// [`Error::with_locale_key`] when you want log-friendly output alongside
    /// the translation tag.
    pub fn localized(status: StatusCode, key: &'static str) -> Self {
        Self {
            status,
            message: key.to_string(),
            source: None,
            error_code: None,
            locale_key: Some(key),
            details: None,
            lagged: false,
        }
    }

    /// Returns the HTTP status code of this error.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the human-readable error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the optional structured details payload.
    pub fn details(&self) -> Option<&serde_json::Value> {
        self.details.as_ref()
    }

    /// Attach a structured JSON details payload (builder-style).
    ///
    /// The payload is rendered under the `"error.details"` key in the JSON
    /// response body produced by [`Error::into_response`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use modo::error::Error;
    /// use modo::serde_json::json;
    ///
    /// let err = Error::unprocessable_entity("validation failed")
    ///     .with_details(json!({ "field": "email", "reason": "invalid format" }));
    /// assert!(err.details().is_some());
    /// ```
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Attach a source error (builder-style).
    ///
    /// The source is stored in a `Box<dyn std::error::Error + Send + Sync>`
    /// and can be downcast with [`Error::source_as`] while you still own the
    /// [`Error`]. Note: the source is **dropped on [`Clone`] and on
    /// [`IntoResponse::into_response`]** — pair `.chain(src)` with
    /// [`.with_code(code)`](Error::with_code) when you need identity that
    /// survives the response boundary.
    ///
    /// # Example
    ///
    /// ```rust
    /// use modo::error::Error;
    /// use std::io;
    ///
    /// let err = Error::internal("disk write failed")
    ///     .chain(io::Error::other("no space"));
    /// assert!(err.source_as::<io::Error>().is_some());
    /// ```
    pub fn chain(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Attach a static error code to preserve error identity through the response pipeline.
    ///
    /// The `source` field is dropped on [`Clone`] and on
    /// [`Error::into_response`], so downstream middleware reading the error
    /// copy from response extensions cannot recover the original cause. A
    /// static `error_code` survives both boundaries and is the canonical way
    /// to identify an error post-response.
    ///
    /// This is a builder method: the existing `message`, `status`, `locale_key`,
    /// `details`, and `source` fields are preserved — only `error_code` is
    /// replaced.
    ///
    /// # Example
    ///
    /// ```rust
    /// use modo::error::Error;
    /// use axum::response::IntoResponse;
    ///
    /// let err = Error::unauthorized("token expired").with_code("jwt:expired");
    /// let resp = err.into_response();
    /// let ext = resp.extensions().get::<Error>().unwrap();
    /// assert_eq!(ext.error_code(), Some("jwt:expired"));
    /// ```
    pub fn with_code(mut self, code: &'static str) -> Self {
        self.error_code = Some(code);
        self
    }

    /// Returns the error code, if one was set.
    pub fn error_code(&self) -> Option<&'static str> {
        self.error_code
    }

    /// Tag an existing error with a translation key (builder-style).
    ///
    /// Unlike [`Error::localized`], this preserves the current `message` — use
    /// it when you already have a descriptive fallback string and want to add a
    /// translation key alongside it. The
    /// [`default_error_handler`](crate::middleware::default_error_handler) will
    /// prefer the translated value whenever a
    /// [`Translator`](crate::i18n::Translator) is available in the request
    /// extensions, and otherwise keep the stored `message` untouched.
    ///
    /// This is a builder method: the existing `message`, `status`, `error_code`,
    /// `details`, and `source` fields are preserved — only `locale_key` is
    /// replaced.
    pub fn with_locale_key(mut self, key: &'static str) -> Self {
        self.locale_key = Some(key);
        self
    }

    /// Returns the translation key, if one was set via [`Error::localized`] or
    /// [`Error::with_locale_key`].
    pub fn locale_key(&self) -> Option<&'static str> {
        self.locale_key
    }

    /// Downcast the source error to a concrete type.
    ///
    /// Returns `None` if no source is set or if the source is not of type `T`.
    pub fn source_as<T: std::error::Error + 'static>(&self) -> Option<&T> {
        self.source.as_ref()?.downcast_ref::<T>()
    }

    /// Create a `400 Bad Request` error.
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, msg)
    }

    /// Create a `401 Unauthorized` error.
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, msg)
    }

    /// Create a `403 Forbidden` error.
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, msg)
    }

    /// Create a `404 Not Found` error.
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, msg)
    }

    /// Create a `409 Conflict` error.
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, msg)
    }

    /// Create a `413 Payload Too Large` error.
    pub fn payload_too_large(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, msg)
    }

    /// Create a `422 Unprocessable Entity` error.
    pub fn unprocessable_entity(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, msg)
    }

    /// Create a `429 Too Many Requests` error.
    pub fn too_many_requests(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, msg)
    }

    /// Create a `500 Internal Server Error`.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, msg)
    }

    /// Create a `502 Bad Gateway` error.
    pub fn bad_gateway(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, msg)
    }

    /// Create a `504 Gateway Timeout` error.
    pub fn gateway_timeout(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::GATEWAY_TIMEOUT, msg)
    }

    /// Create an error indicating a broadcast subscriber lagged behind.
    ///
    /// The resulting error has a `500 Internal Server Error` status and [`is_lagged`](Error::is_lagged)
    /// returns `true`. `skipped` is the number of messages that were dropped.
    pub fn lagged(skipped: u64) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("SSE subscriber lagged, skipped {skipped} messages"),
            source: None,
            error_code: None,
            locale_key: None,
            details: None,
            lagged: true,
        }
    }

    /// Returns `true` if this error represents a broadcast lag.
    pub fn is_lagged(&self) -> bool {
        self.lagged
    }
}

/// Clones the error, dropping the `source` field (which is not `Clone`).
///
/// All other fields — `status`, `message`, `error_code`, `locale_key`, `details`, and
/// `lagged` — are preserved.
impl Clone for Error {
    fn clone(&self) -> Self {
        Self {
            status: self.status,
            message: self.message.clone(),
            source: None, // source (Box<dyn Error>) can't be cloned
            error_code: self.error_code,
            locale_key: self.locale_key,
            details: self.details.clone(),
            lagged: self.lagged,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("status", &self.status)
            .field("message", &self.message)
            .field("source", &self.source)
            .field("error_code", &self.error_code)
            .field("locale_key", &self.locale_key)
            .field("details", &self.details)
            .field("lagged", &self.lagged)
            .finish()
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

/// Builds the JSON body shared by [`Error::into_response`] and
/// [`default_error_handler`](crate::middleware::default_error_handler).
///
/// Produces `{"error": {"status", "message"}}`, with a nested
/// `"details"` key only when `details` is `Some`. Keeping this in one place
/// ensures the two code paths stay byte-identical.
pub(crate) fn render_error_body(
    status: StatusCode,
    message: &str,
    details: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "error": {
            "status": status.as_u16(),
            "message": message,
        }
    });
    if let Some(d) = details {
        body["error"]["details"] = d.clone();
    }
    body
}

/// Converts `Error` into an axum [`Response`].
///
/// Produces a JSON body of the form:
///
/// ```json
/// { "error": { "status": 422, "message": "validation failed" } }
/// ```
///
/// If [`with_details`](Error::with_details) was called, a `"details"` key is added under `"error"`.
///
/// A copy of the error (without the `source` field) is stored in response extensions under
/// the type `Error` so that downstream middleware can inspect it.
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = self.status;
        let message = self.message.clone();
        let details = self.details.clone();

        let body = render_error_body(status, &message, details.as_ref());

        // Store a copy in extensions so error_handler middleware can read it
        let ext_error = Error {
            status,
            message,
            source: None, // source can't be cloned
            error_code: self.error_code,
            locale_key: self.locale_key,
            details,
            lagged: self.lagged,
        };

        let mut response = (status, axum::Json(body)).into_response();
        response.extensions_mut().insert(ext_error);
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lagged_error_has_internal_status() {
        let err = Error::lagged(5);
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(err.message().contains('5'));
    }

    #[test]
    fn is_lagged_returns_true_for_lagged_error() {
        let err = Error::lagged(10);
        assert!(err.is_lagged());
    }

    #[test]
    fn is_lagged_returns_false_for_other_errors() {
        let err = Error::internal("something else");
        assert!(!err.is_lagged());
    }

    #[test]
    fn payload_too_large_error_has_413_status() {
        let err = Error::payload_too_large("file too big");
        assert_eq!(err.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(err.message(), "file too big");
    }

    #[test]
    fn chain_sets_source() {
        use std::error::Error as _;
        use std::io;
        let err = super::Error::internal("failed").chain(io::Error::other("disk"));
        assert!(err.source().is_some());
    }

    #[test]
    fn source_as_downcasts_correctly() {
        use std::io;
        let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
        let err = Error::internal("failed").chain(io_err);
        let downcasted = err.source_as::<io::Error>();
        assert!(downcasted.is_some());
        assert_eq!(downcasted.unwrap().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn source_as_returns_none_for_wrong_type() {
        use std::io;
        let err = Error::internal("failed").chain(io::Error::other("x"));
        let downcasted = err.source_as::<std::num::ParseIntError>();
        assert!(downcasted.is_none());
    }

    #[test]
    fn source_as_returns_none_when_no_source() {
        let err = Error::internal("no source");
        let downcasted = err.source_as::<std::io::Error>();
        assert!(downcasted.is_none());
    }

    #[test]
    fn with_code_sets_error_code() {
        let err = Error::unauthorized("denied").with_code("jwt:expired");
        assert_eq!(err.error_code(), Some("jwt:expired"));
    }

    #[test]
    fn error_code_is_none_by_default() {
        let err = Error::internal("plain");
        assert!(err.error_code().is_none());
    }

    #[test]
    fn error_code_survives_clone() {
        let err = Error::unauthorized("denied").with_code("jwt:expired");
        let cloned = err.clone();
        assert_eq!(cloned.error_code(), Some("jwt:expired"));
    }

    #[test]
    fn error_code_survives_into_response() {
        use axum::response::IntoResponse;
        let err = Error::unauthorized("denied").with_code("jwt:expired");
        let response = err.into_response();
        let ext_err = response.extensions().get::<Error>().unwrap();
        assert_eq!(ext_err.error_code(), Some("jwt:expired"));
    }

    #[test]
    fn bad_gateway_error_has_502_status() {
        let err = Error::bad_gateway("upstream failed");
        assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.message(), "upstream failed");
    }

    #[test]
    fn gateway_timeout_error_has_504_status() {
        let err = Error::gateway_timeout("timed out");
        assert_eq!(err.status(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(err.message(), "timed out");
    }

    #[test]
    fn localized_sets_key_and_falls_back_to_key_as_message() {
        let err = Error::localized(StatusCode::NOT_FOUND, "errors.user.not_found");
        assert_eq!(err.status(), StatusCode::NOT_FOUND);
        assert_eq!(err.locale_key(), Some("errors.user.not_found"));
        // Fallback message equals the key itself so responses remain predictable
        // when no error-handler middleware / Translator is installed.
        assert_eq!(err.message(), "errors.user.not_found");
        assert!(err.error_code().is_none());
        assert!(err.details().is_none());
    }

    #[test]
    fn with_locale_key_tags_existing_error() {
        let err = Error::bad_request("boom").with_locale_key("errors.validation.generic");
        // Builder must preserve the existing message, only attach the key.
        assert_eq!(err.message(), "boom");
        assert_eq!(err.locale_key(), Some("errors.validation.generic"));
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn clone_preserves_locale_key() {
        let err = Error::localized(StatusCode::CONFLICT, "errors.email.in_use");
        let cloned = err.clone();
        assert_eq!(cloned.locale_key(), Some("errors.email.in_use"));
        assert_eq!(cloned.status(), StatusCode::CONFLICT);
        assert_eq!(cloned.message(), "errors.email.in_use");
    }

    #[test]
    fn response_extensions_clone_preserves_locale_key() {
        use axum::response::IntoResponse;
        let err = Error::localized(StatusCode::UNAUTHORIZED, "errors.auth.expired");
        let response = err.into_response();
        let ext_err = response.extensions().get::<Error>().unwrap();
        assert_eq!(ext_err.locale_key(), Some("errors.auth.expired"));
        assert_eq!(ext_err.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn render_error_body_without_details() {
        let body = render_error_body(StatusCode::NOT_FOUND, "user not found", None);
        assert_eq!(
            body,
            serde_json::json!({
                "error": {
                    "status": 404,
                    "message": "user not found",
                }
            })
        );
    }

    #[test]
    fn render_error_body_with_details() {
        let details = serde_json::json!({"field": "email"});
        let body = render_error_body(StatusCode::UNPROCESSABLE_ENTITY, "invalid", Some(&details));
        assert_eq!(
            body,
            serde_json::json!({
                "error": {
                    "status": 422,
                    "message": "invalid",
                    "details": {"field": "email"},
                }
            })
        );
    }
}
