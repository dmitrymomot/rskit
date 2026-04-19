use http::{HeaderName, HeaderValue, Request};
use tower::layer::util::{Identity, Stack};
use tower_http::request_id::{
    MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer,
};

static X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// ULID-based request ID generator used internally by [`request_id`].
#[derive(Clone)]
pub struct ModoRequestId;

impl MakeRequestId for ModoRequestId {
    fn make_request_id<B>(&mut self, _request: &Request<B>) -> Option<RequestId> {
        let id = crate::id::ulid();
        Some(RequestId::new(HeaderValue::from_str(&id).unwrap()))
    }
}

/// Returns a layer that sets and propagates an `x-request-id` header.
///
/// If the incoming request already has an `x-request-id` header, it is
/// preserved — allowing an upstream proxy to correlate requests across
/// services. Otherwise a new ULID (26 chars, via [`crate::id::ulid`]) is
/// generated and set on the request. The response always carries the
/// `x-request-id` header.
///
/// # Layer ordering
///
/// Install `request_id()` **outside** handler-facing layers so the ID is
/// attached before any handler or authorization layer runs, and **inside**
/// [`tracing`](super::tracing) so the span can record it. The
/// returned [`tower::ServiceBuilder`] bundles `SetRequestIdLayer` +
/// `PropagateRequestIdLayer`, so a single `.layer(request_id())` call
/// installs both halves.
///
/// # Example
///
/// ```rust,no_run
/// use axum::{Router, routing::get};
/// use modo::middleware::request_id;
///
/// let app: Router = Router::new()
///     .route("/", get(|| async { "ok" }))
///     .layer(request_id());
/// ```
pub fn request_id() -> tower::ServiceBuilder<
    Stack<PropagateRequestIdLayer, Stack<SetRequestIdLayer<ModoRequestId>, Identity>>,
> {
    tower::ServiceBuilder::new()
        .layer(SetRequestIdLayer::new(X_REQUEST_ID.clone(), ModoRequestId))
        .layer(PropagateRequestIdLayer::new(X_REQUEST_ID.clone()))
}
