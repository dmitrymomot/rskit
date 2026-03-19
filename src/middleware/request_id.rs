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
/// If the incoming request already has an `x-request-id` header, it is preserved.
/// Otherwise, a new ULID is generated and set on the request. The response always
/// includes the `x-request-id` header.
pub fn request_id() -> tower::ServiceBuilder<
    Stack<PropagateRequestIdLayer, Stack<SetRequestIdLayer<ModoRequestId>, Identity>>,
> {
    tower::ServiceBuilder::new()
        .layer(SetRequestIdLayer::new(X_REQUEST_ID.clone(), ModoRequestId))
        .layer(PropagateRequestIdLayer::new(X_REQUEST_ID.clone()))
}
