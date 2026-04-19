use tower_http::compression::CompressionLayer;

/// Returns a compression layer that automatically compresses response bodies.
///
/// Supports gzip, deflate, brotli, and zstd, selected from the client's
/// `Accept-Encoding` header. If the client offers no supported encoding
/// the response body is passed through unchanged.
///
/// # Layer ordering
///
/// Install `compression()` **inside** [`tracing`](super::tracing) and
/// [`request_id`](super::request_id) so the compressed bytes are never
/// observed by those layers, but **outside** (i.e. wrapping) the handler
/// so the handler always writes a plain body.
///
/// # Example
///
/// ```rust,no_run
/// use axum::{Router, routing::get};
/// use modo::middleware::compression;
///
/// let app: Router = Router::new()
///     .route("/", get(|| async { "hello" }))
///     .layer(compression());
/// ```
pub fn compression() -> CompressionLayer {
    CompressionLayer::new()
}
