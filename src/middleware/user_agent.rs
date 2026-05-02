use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use http::header::USER_AGENT;
use http::{HeaderValue, Request};
use tower::{Layer, Service};

/// Default sanitization cap, in bytes.
///
/// Real-world `User-Agent` strings are typically 100–300 bytes. 512 leaves
/// headroom for verbose UA brand reduction strings without permitting the
/// pathological multi-kilobyte values an upstream HTTP server will otherwise
/// accept.
const DEFAULT_MAX_LEN: usize = 512;

/// Tower layer that sanitizes the inbound `User-Agent` header in place.
///
/// Reads the request's `User-Agent` once, applies a small set of
/// normalization rules (length cap on a UTF-8 char boundary, ASCII control
/// stripping, whitespace collapsing, trimming), and rewrites the header value
/// before delegating to the inner service. If the sanitized result is empty
/// the header is removed entirely so downstream consumers see the same
/// "missing" state they handle today.
///
/// Because the layer mutates the request header itself, every downstream
/// reader — `ClientInfo`, the cookie session middleware, audit logging,
/// fingerprint hashing — observes the cleaned value with no further plumbing.
///
/// # Layer ordering
///
/// Install `UserAgentLayer` **before** any layer or handler that reads
/// `User-Agent`. In axum that means it must be added with a `.layer(...)`
/// call that comes **after** (i.e. wraps) the consumer:
///
/// ```rust,no_run
/// use axum::{Router, routing::get};
/// use modo::middleware::UserAgentLayer;
///
/// let app: Router = Router::new()
///     .route("/", get(|| async { "ok" }))
///     .layer(UserAgentLayer::new());
/// ```
#[derive(Debug, Clone, Copy)]
pub struct UserAgentLayer {
    max_len: usize,
}

impl UserAgentLayer {
    /// Create a layer with the default 512-byte sanitization cap.
    pub fn new() -> Self {
        Self {
            max_len: DEFAULT_MAX_LEN,
        }
    }

    /// Create a layer with a custom byte-length cap.
    pub fn with_max_length(max_len: usize) -> Self {
        Self { max_len }
    }
}

impl Default for UserAgentLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Layer<S> for UserAgentLayer {
    type Service = UserAgentMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        UserAgentMiddleware {
            inner,
            max_len: self.max_len,
        }
    }
}

/// Tower service produced by [`UserAgentLayer`].
pub struct UserAgentMiddleware<S> {
    inner: S,
    max_len: usize,
}

impl<S: Clone> Clone for UserAgentMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            max_len: self.max_len,
        }
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for UserAgentMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = http::Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqBody>) -> Self::Future {
        let max_len = self.max_len;
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            // Snapshot the existing header value as an owned string so we can
            // compare it to the sanitized output without juggling lifetimes
            // against `headers_mut()`.
            let raw = request
                .headers()
                .get(USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);

            if let Some(raw) = raw {
                match sanitize_user_agent(&raw, max_len) {
                    Some(clean) => {
                        // Sanitization output is a subset of an already
                        // visible-ASCII input, so `from_str` cannot fail.
                        let value = HeaderValue::from_str(&clean)
                            .expect("sanitized user-agent must be a valid header value");
                        // `insert` replaces every existing entry, so a request
                        // arriving with multiple `User-Agent` headers (rare but
                        // valid HTTP) is normalized down to a single value.
                        request.headers_mut().insert(USER_AGENT, value);
                    }
                    None => {
                        request.headers_mut().remove(USER_AGENT);
                    }
                }
            }

            inner.call(request).await
        })
    }
}

/// Sanitize a raw `User-Agent` value.
///
/// 1. Truncate to `max_len` bytes, snapping down to the nearest UTF-8 char
///    boundary so the result is always valid UTF-8.
/// 2. Drop ASCII control characters.
/// 3. Collapse runs of ASCII whitespace into a single space. Note that
///    [`char::is_ascii_whitespace`] includes tab, newline, carriage return,
///    and form-feed — those bytes never reach the layer's call site
///    (`HeaderValue::to_str` rejects all but tab), but they are accepted
///    here because the function is `pub(crate)` and may be called directly.
/// 4. Trim leading and trailing whitespace.
/// 5. Return `None` if the resulting string is empty.
pub(crate) fn sanitize_user_agent(raw: &str, max_len: usize) -> Option<String> {
    let mut end = raw.len().min(max_len);
    while end > 0 && !raw.is_char_boundary(end) {
        end -= 1;
    }
    let truncated = &raw[..end];

    let mut out = String::with_capacity(truncated.len());
    let mut prev_ws = false;
    for c in truncated.chars() {
        if c.is_ascii_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
            continue;
        }
        if c.is_ascii_control() {
            // Intentionally do not reset `prev_ws`: a control char between
            // two spaces still collapses to a single space.
            continue;
        }
        out.push(c);
        prev_ws = false;
    }

    let trimmed = out.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.len() == out.len() {
        Some(out)
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::{Request, Response, StatusCode};
    use std::convert::Infallible;
    use tower::ServiceExt;

    // ---------- sanitize_user_agent ----------

    #[test]
    fn passes_clean_short_ua() {
        assert_eq!(
            sanitize_user_agent("Mozilla/5.0", 512).as_deref(),
            Some("Mozilla/5.0"),
        );
    }

    #[test]
    fn truncates_to_max_len_ascii() {
        let raw: String = "A".repeat(1024);
        let out = sanitize_user_agent(&raw, 64).unwrap();
        assert_eq!(out.len(), 64);
        assert!(out.chars().all(|c| c == 'A'));
    }

    #[test]
    fn truncates_at_char_boundary_multibyte() {
        // "ñ" is 2 bytes in UTF-8.
        let raw: String = "ñ".repeat(20);
        // Cap at an odd byte count that lands inside a multibyte char.
        let out = sanitize_user_agent(&raw, 5).unwrap();
        assert!(out.len() <= 5);
        // Must still be valid UTF-8 (would have panicked above otherwise).
        assert!(out.chars().all(|c| c == 'ñ'));
        assert_eq!(out.len() % 2, 0);
    }

    #[test]
    fn strips_ascii_control_chars() {
        let out = sanitize_user_agent("Mozilla\x01/\x07X", 512).unwrap();
        assert_eq!(out, "Mozilla/X");
    }

    #[test]
    fn collapses_whitespace_runs() {
        let out = sanitize_user_agent("Mozilla   \t  /5.0", 512).unwrap();
        assert_eq!(out, "Mozilla /5.0");
    }

    #[test]
    fn trims_leading_and_trailing_whitespace() {
        assert_eq!(
            sanitize_user_agent("   UA-Test   ", 512).as_deref(),
            Some("UA-Test"),
        );
    }

    #[test]
    fn keeps_non_ascii_chars() {
        // Non-ASCII passes through untouched (HeaderValue::to_str() at the
        // call site already rejected anything that isn't visible ASCII, so in
        // practice this is defensive — but the sanitizer itself is permissive
        // for non-ASCII so it can be reused outside the layer).
        assert_eq!(
            sanitize_user_agent("клиент/1.0", 512).as_deref(),
            Some("клиент/1.0"),
        );
    }

    #[test]
    fn returns_none_for_empty_input() {
        assert!(sanitize_user_agent("", 512).is_none());
    }

    #[test]
    fn returns_none_for_only_whitespace() {
        assert!(sanitize_user_agent("   \t  ", 512).is_none());
    }

    #[test]
    fn returns_none_for_only_controls() {
        assert!(sanitize_user_agent("\x01\x02\x03", 512).is_none());
    }

    #[test]
    fn zero_max_len_returns_none() {
        assert!(sanitize_user_agent("Mozilla/5.0", 0).is_none());
    }

    // ---------- UserAgentLayer / Service ----------

    async fn echo_ua(req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let ua = req
            .headers()
            .get(USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .unwrap_or_else(|| "<absent>".to_string());
        Ok(Response::new(Body::from(ua)))
    }

    async fn run(svc_layer: UserAgentLayer, req: Request<Body>) -> String {
        let svc = svc_layer.layer(tower::service_fn(echo_ua));
        let resp = svc.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(body.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn passes_clean_ua_unchanged() {
        let req = Request::builder()
            .header(USER_AGENT, "Mozilla/5.0")
            .body(Body::empty())
            .unwrap();
        assert_eq!(run(UserAgentLayer::new(), req).await, "Mozilla/5.0");
    }

    #[tokio::test]
    async fn truncates_long_ua() {
        let long = "A".repeat(2000);
        let req = Request::builder()
            .header(USER_AGENT, long)
            .body(Body::empty())
            .unwrap();
        let out = run(UserAgentLayer::with_max_length(64), req).await;
        assert_eq!(out.len(), 64);
        assert!(out.chars().all(|c| c == 'A'));
    }

    #[tokio::test]
    async fn strips_controls_and_collapses_whitespace() {
        // Tab is the only control char `HeaderValue::to_str()` accepts in
        // addition to visible ASCII, so this is the realistic shape of an
        // attacker-controlled header.
        let req = Request::builder()
            .header(USER_AGENT, "Mozilla/5.0\t\t (foo) bar")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            run(UserAgentLayer::new(), req).await,
            "Mozilla/5.0 (foo) bar",
        );
    }

    #[tokio::test]
    async fn removes_header_when_only_whitespace() {
        let req = Request::builder()
            .header(USER_AGENT, "   \t  ")
            .body(Body::empty())
            .unwrap();
        assert_eq!(run(UserAgentLayer::new(), req).await, "<absent>");
    }

    #[tokio::test]
    async fn leaves_absent_header_alone() {
        let req = Request::builder().body(Body::empty()).unwrap();
        assert_eq!(run(UserAgentLayer::new(), req).await, "<absent>");
    }

    #[tokio::test]
    async fn respects_with_max_length() {
        let req = Request::builder()
            .header(USER_AGENT, "abcdefghijklmnop")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            run(UserAgentLayer::with_max_length(8), req).await,
            "abcdefgh"
        );
    }

    #[tokio::test]
    async fn normalizes_duplicate_user_agent_headers() {
        // Multiple `User-Agent` headers are rare but valid HTTP. The layer
        // should always reduce them to a single value so downstream consumers
        // never see duplicates regardless of whether the first value needed
        // sanitization.
        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.headers_mut()
            .append(USER_AGENT, "Mozilla/5.0".parse().unwrap());
        req.headers_mut()
            .append(USER_AGENT, "Other/1.0".parse().unwrap());

        let svc = UserAgentLayer::new().layer(tower::service_fn(|req: Request<Body>| async move {
            let count = req.headers().get_all(USER_AGENT).iter().count();
            let first = req
                .headers()
                .get(USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            Ok::<_, Infallible>(Response::new(Body::from(format!("{count}|{first}"))))
        }));
        let resp = svc.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"1|Mozilla/5.0");
    }
}
