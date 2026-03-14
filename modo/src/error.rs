use axum::Json;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tracing;

/// All standard HTTP 4xx and 5xx error variants.
/// Used for ergonomic error construction: `HttpError::NotFound.into()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpError {
    // 4xx
    BadRequest,
    Unauthorized,
    PaymentRequired,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    NotAcceptable,
    ProxyAuthRequired,
    RequestTimeout,
    Conflict,
    Gone,
    LengthRequired,
    PreconditionFailed,
    PayloadTooLarge,
    UriTooLong,
    UnsupportedMediaType,
    RangeNotSatisfiable,
    ExpectationFailed,
    ImATeapot,
    MisdirectedRequest,
    UnprocessableEntity,
    Locked,
    FailedDependency,
    TooEarly,
    UpgradeRequired,
    PreconditionRequired,
    TooManyRequests,
    HeaderFieldsTooLarge,
    UnavailableForLegalReasons,
    // 5xx
    InternalServerError,
    NotImplemented,
    BadGateway,
    ServiceUnavailable,
    GatewayTimeout,
    HttpVersionNotSupported,
    VariantAlsoNegotiates,
    InsufficientStorage,
    LoopDetected,
    NotExtended,
    NetworkAuthRequired,
}

macro_rules! http_error_mappings {
    ($($variant:ident => ($status:expr, $code:literal, $message:literal)),+ $(,)?) => {
        impl HttpError {
            pub fn status_code(&self) -> StatusCode {
                match self {
                    $(Self::$variant => $status,)+
                }
            }

            pub fn code(&self) -> &'static str {
                match self {
                    $(Self::$variant => $code,)+
                }
            }

            pub fn message(&self) -> &'static str {
                match self {
                    $(Self::$variant => $message,)+
                }
            }
        }
    };
}

http_error_mappings! {
    // 4xx
    BadRequest           => (StatusCode::BAD_REQUEST,            "bad_request",            "Bad request"),
    Unauthorized         => (StatusCode::UNAUTHORIZED,           "unauthorized",           "Unauthorized"),
    PaymentRequired      => (StatusCode::PAYMENT_REQUIRED,       "payment_required",       "Payment required"),
    Forbidden            => (StatusCode::FORBIDDEN,              "forbidden",              "Forbidden"),
    NotFound             => (StatusCode::NOT_FOUND,              "not_found",              "Not found"),
    MethodNotAllowed     => (StatusCode::METHOD_NOT_ALLOWED,     "method_not_allowed",     "Method not allowed"),
    NotAcceptable        => (StatusCode::NOT_ACCEPTABLE,         "not_acceptable",         "Not acceptable"),
    ProxyAuthRequired    => (StatusCode::PROXY_AUTHENTICATION_REQUIRED, "proxy_auth_required", "Proxy authentication required"),
    RequestTimeout       => (StatusCode::REQUEST_TIMEOUT,        "request_timeout",        "Request timeout"),
    Conflict             => (StatusCode::CONFLICT,               "conflict",               "Conflict"),
    Gone                 => (StatusCode::GONE,                   "gone",                   "Gone"),
    LengthRequired       => (StatusCode::LENGTH_REQUIRED,        "length_required",        "Length required"),
    PreconditionFailed   => (StatusCode::PRECONDITION_FAILED,    "precondition_failed",    "Precondition failed"),
    PayloadTooLarge      => (StatusCode::PAYLOAD_TOO_LARGE,      "payload_too_large",      "Payload too large"),
    UriTooLong           => (StatusCode::URI_TOO_LONG,           "uri_too_long",           "URI too long"),
    UnsupportedMediaType => (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type", "Unsupported media type"),
    RangeNotSatisfiable  => (StatusCode::RANGE_NOT_SATISFIABLE,  "range_not_satisfiable",  "Range not satisfiable"),
    ExpectationFailed    => (StatusCode::EXPECTATION_FAILED,     "expectation_failed",     "Expectation failed"),
    ImATeapot            => (StatusCode::IM_A_TEAPOT,            "im_a_teapot",            "I'm a teapot"),
    MisdirectedRequest   => (StatusCode::MISDIRECTED_REQUEST,    "misdirected_request",    "Misdirected request"),
    UnprocessableEntity  => (StatusCode::UNPROCESSABLE_ENTITY,   "unprocessable_entity",   "Unprocessable entity"),
    Locked               => (StatusCode::LOCKED,                 "locked",                 "Locked"),
    FailedDependency     => (StatusCode::FAILED_DEPENDENCY,      "failed_dependency",      "Failed dependency"),
    TooEarly             => (StatusCode::TOO_EARLY,              "too_early",              "Too early"),
    UpgradeRequired      => (StatusCode::UPGRADE_REQUIRED,       "upgrade_required",       "Upgrade required"),
    PreconditionRequired => (StatusCode::PRECONDITION_REQUIRED,  "precondition_required",  "Precondition required"),
    TooManyRequests      => (StatusCode::TOO_MANY_REQUESTS,      "too_many_requests",      "Too many requests"),
    HeaderFieldsTooLarge => (StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE, "header_fields_too_large", "Request header fields too large"),
    UnavailableForLegalReasons => (StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS, "unavailable_for_legal_reasons", "Unavailable for legal reasons"),
    // 5xx
    InternalServerError     => (StatusCode::INTERNAL_SERVER_ERROR,     "internal_server_error",      "Internal server error"),
    NotImplemented          => (StatusCode::NOT_IMPLEMENTED,           "not_implemented",            "Not implemented"),
    BadGateway              => (StatusCode::BAD_GATEWAY,               "bad_gateway",                "Bad gateway"),
    ServiceUnavailable      => (StatusCode::SERVICE_UNAVAILABLE,       "service_unavailable",        "Service unavailable"),
    GatewayTimeout          => (StatusCode::GATEWAY_TIMEOUT,           "gateway_timeout",            "Gateway timeout"),
    HttpVersionNotSupported => (StatusCode::HTTP_VERSION_NOT_SUPPORTED, "http_version_not_supported", "HTTP version not supported"),
    VariantAlsoNegotiates   => (StatusCode::VARIANT_ALSO_NEGOTIATES,   "variant_also_negotiates",    "Variant also negotiates"),
    InsufficientStorage     => (StatusCode::INSUFFICIENT_STORAGE,      "insufficient_storage",       "Insufficient storage"),
    LoopDetected            => (StatusCode::LOOP_DETECTED,             "loop_detected",              "Loop detected"),
    NotExtended             => (StatusCode::NOT_EXTENDED,              "not_extended",               "Not extended"),
    NetworkAuthRequired     => (StatusCode::NETWORK_AUTHENTICATION_REQUIRED, "network_auth_required", "Network authentication required"),
}

impl HttpError {
    /// Create an `Error` from this variant with a custom message.
    pub fn with_message(self, msg: impl Into<String>) -> Error {
        Error {
            status: self.status_code(),
            code: self.code().to_owned(),
            message: msg.into(),
            details: HashMap::new(),
            source: None,
        }
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        Error::from(self).into_response()
    }
}

/// A structured HTTP error with status, code, message, and flexible details.
#[derive(Debug, Clone)]
pub struct Error {
    status: StatusCode,
    code: String,
    message: String,
    details: HashMap<String, serde_json::Value>,
    source: Option<Arc<dyn std::error::Error + Send + Sync>>,
}

impl Error {
    pub fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
            details: HashMap::new(),
            source: None,
        }
    }

    /// Convenience: creates an `InternalServerError` with a custom message.
    pub fn internal(msg: impl Into<String>) -> Self {
        HttpError::InternalServerError.with_message(msg)
    }

    /// Creates an `InternalServerError` with code `"panic"` for caught panics.
    pub fn internal_panic(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "panic".to_owned(),
            message: msg.into(),
            details: HashMap::new(),
            source: None,
        }
    }

    pub fn message(mut self, msg: impl Into<String>) -> Self {
        self.message = msg.into();
        self
    }

    pub fn detail(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.details.insert(key.into(), value);
        self
    }

    pub fn with_source(mut self, err: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Arc::new(err));
        self
    }

    pub fn status_code(&self) -> StatusCode {
        self.status
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message_str(&self) -> &str {
        &self.message
    }

    pub fn details(&self) -> &HashMap<String, serde_json::Value> {
        &self.details
    }

    /// Renders the default JSON response for this error.
    /// Custom error handlers can call this to delegate back to the default rendering.
    pub fn default_response(&self) -> Response {
        let status = self.status;

        if status.is_server_error() {
            tracing::error!(
                status = status.as_u16(),
                code = %self.code,
                message = %self.message,
                source = ?self.source,
                "Server error"
            );

            let http = HttpError::InternalServerError;
            let body = Json(json!({
                "error": http.code(),
                "message": http.message(),
                "status": status.as_u16(),
            }));
            return (status, body).into_response();
        }

        let mut body = json!({
            "error": self.code,
            "message": self.message,
            "status": status.as_u16(),
        });
        if !self.details.is_empty() {
            body["details"] = json!(self.details);
        }
        let body = Json(body);

        (status, body).into_response()
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|s| s.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let mut response = self.default_response();
        response.extensions_mut().insert(self);
        response
    }
}

impl From<HttpError> for Error {
    fn from(http: HttpError) -> Self {
        Self {
            status: http.status_code(),
            code: http.code().to_owned(),
            message: http.message().to_owned(),
            details: HashMap::new(),
            source: None,
        }
    }
}

#[cfg(feature = "templates")]
impl From<crate::templates::TemplateError> for Error {
    fn from(e: crate::templates::TemplateError) -> Self {
        Error::internal(format!("Template render failed: {e}"))
    }
}

#[cfg(feature = "i18n")]
impl From<crate::i18n::I18nError> for Error {
    fn from(e: crate::i18n::I18nError) -> Self {
        Error::internal(e.to_string())
    }
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        let message = err.to_string();
        // anyhow::Error doesn't implement std::error::Error, so convert via into_boxed()
        let boxed: Box<dyn std::error::Error + Send + Sync> = err.into();
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_server_error".to_owned(),
            message,
            details: HashMap::new(),
            source: Some(Arc::from(boxed)),
        }
    }
}

/// Request context passed to custom error handlers for content negotiation.
pub struct ErrorContext {
    pub method: Method,
    pub uri: Uri,
    pub headers: HeaderMap,
}

impl ErrorContext {
    /// Returns `true` if the `Accept` header contains `text/html`.
    pub fn accepts_html(&self) -> bool {
        self.headers
            .get("accept")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("text/html"))
    }

    /// Returns `true` if the request was sent by HTMX (`HX-Request` header present).
    pub fn is_htmx(&self) -> bool {
        self.headers.get("hx-request").is_some()
    }
}

/// Convenience alias for JSON API handlers.
/// Defaults to `Result<axum::Json<T>, Error>`, but the error type can be overridden.
pub type JsonResult<T, E = Error> = Result<axum::Json<T>, E>;

/// Convenience alias for generic handler results.
/// Defaults to `Result<T, Error>`, but the error type can be overridden.
pub type HandlerResult<T, E = Error> = Result<T, E>;

/// Result type for handlers that use `ViewRenderer`.
/// Supports rendering views, composing multiple views, and smart redirects.
#[cfg(feature = "templates")]
pub type ViewResult<E = Error> = Result<crate::templates::ViewResponse, E>;

/// Signature for custom error handler functions.
pub type ErrorHandlerFn = fn(Error, &ErrorContext) -> Response;

/// Registration entry for a custom error handler, collected via `inventory`.
pub struct ErrorHandlerRegistration {
    pub handler: ErrorHandlerFn,
}

inventory::collect!(ErrorHandlerRegistration);

/// Middleware that intercepts `Error` extensions on responses and delegates
/// to a registered custom error handler if one exists.
pub async fn error_handler_middleware(
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    let ctx = ErrorContext {
        method: request.method().clone(),
        uri: request.uri().clone(),
        headers: request.headers().clone(),
    };
    let mut response = next.run(request).await;
    if let Some(error) = response.extensions_mut().remove::<Error>()
        && let Some(reg) = inventory::iter::<ErrorHandlerRegistration>
            .into_iter()
            .next()
    {
        return (reg.handler)(error, &ctx);
    }
    response
}
