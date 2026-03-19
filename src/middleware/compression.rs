use tower_http::compression::CompressionLayer;

/// Returns a compression layer that automatically compresses response bodies.
///
/// Supports gzip, deflate, brotli, and zstd based on the `Accept-Encoding` header.
pub fn compression() -> CompressionLayer {
    CompressionLayer::new()
}
