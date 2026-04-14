use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use cookie::{Cookie, CookieJar, SameSite};
use http::{HeaderValue, Request, Response};
use tower::{Layer, Service};

use crate::cookie::{CookieConfig, Key};

use super::state::{FlashEntry, FlashState};

const COOKIE_NAME: &str = "flash";
const MAX_AGE_SECS: i64 = 300;

// --- Layer ---

/// Tower [`Layer`] that enables cookie-based flash messages for a router.
///
/// On each request the layer reads the signed `flash` cookie and populates
/// the [`Flash`](crate::flash::Flash) extractor. On response it either writes a new
/// signed cookie (when messages were queued) or removes the existing one (when
/// messages were consumed via [`Flash::messages`](crate::flash::Flash::messages)).
///
/// # Cookie details
///
/// - Name: `flash`
/// - Signed with HMAC using the application [`Key`]
/// - `Max-Age`: 300 seconds (5 minutes)
/// - Path, `Secure`, `HttpOnly`, and `SameSite` attributes come from [`CookieConfig`]
pub struct FlashLayer {
    key: Key,
    config: CookieConfig,
}

impl Clone for FlashLayer {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            config: self.config.clone(),
        }
    }
}

impl FlashLayer {
    /// Create a new `FlashLayer` from a cookie configuration and signing key.
    pub fn new(config: &CookieConfig, key: &Key) -> Self {
        Self {
            key: key.clone(),
            config: config.clone(),
        }
    }
}

impl<S> Layer<S> for FlashLayer {
    type Service = FlashMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        FlashMiddleware {
            inner,
            key: self.key.clone(),
            config: self.config.clone(),
        }
    }
}

// --- Service ---

/// Tower [`Service`] produced by [`FlashLayer`].
pub struct FlashMiddleware<S> {
    inner: S,
    key: Key,
    config: CookieConfig,
}

impl<S: Clone> Clone for FlashMiddleware<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            key: self.key.clone(),
            config: self.config.clone(),
        }
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for FlashMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<ReqBody>) -> Self::Future {
        let key = self.key.clone();
        let config = self.config.clone();
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            // --- Request path ---
            let incoming = read_flash_cookie(request.headers(), &key);
            let flash_state = Arc::new(FlashState::new(incoming));
            request.extensions_mut().insert(flash_state.clone());

            // --- Run inner service ---
            let mut response = inner.call(request).await?;

            // --- Response path ---
            let outgoing = flash_state.drain_outgoing();
            let was_read = flash_state.was_read();

            if !outgoing.is_empty() {
                write_flash_cookie(&mut response, &outgoing, &config, &key);
            } else if was_read {
                remove_flash_cookie(&mut response, &config, &key);
            }

            Ok(response)
        })
    }
}

fn read_flash_cookie(headers: &http::HeaderMap, key: &Key) -> Vec<FlashEntry> {
    let Some(cookie_header) = headers.get(http::header::COOKIE) else {
        return vec![];
    };
    let Ok(cookie_str) = cookie_header.to_str() else {
        return vec![];
    };

    for pair in cookie_str.split(';') {
        let pair = pair.trim();
        if let Some((name, value)) = pair.split_once('=')
            && name.trim() == COOKIE_NAME
        {
            let mut jar = CookieJar::new();
            jar.add_original(Cookie::new(
                COOKIE_NAME.to_string(),
                value.trim().to_string(),
            ));
            if let Some(verified) = jar.signed(key).get(COOKIE_NAME)
                && let Ok(entries) = serde_json::from_str::<Vec<FlashEntry>>(verified.value())
            {
                return entries;
            }
            return vec![];
        }
    }
    vec![]
}

fn write_flash_cookie(
    response: &mut Response<Body>,
    entries: &[FlashEntry],
    config: &CookieConfig,
    key: &Key,
) {
    let Ok(json) = serde_json::to_string(entries) else {
        tracing::error!("failed to serialize flash messages");
        return;
    };

    set_cookie(response, &json, MAX_AGE_SECS, config, key);
}

fn remove_flash_cookie(response: &mut Response<Body>, config: &CookieConfig, key: &Key) {
    set_cookie(response, "", 0, config, key);
}

fn set_cookie(
    response: &mut Response<Body>,
    value: &str,
    max_age_secs: i64,
    config: &CookieConfig,
    key: &Key,
) {
    let mut jar = CookieJar::new();
    jar.signed_mut(key)
        .add(Cookie::new(COOKIE_NAME.to_string(), value.to_string()));
    let signed_value = jar
        .get(COOKIE_NAME)
        .expect("cookie was just added")
        .value()
        .to_string();

    let same_site = match config.same_site.as_str() {
        "strict" => SameSite::Strict,
        "none" => SameSite::None,
        _ => SameSite::Lax,
    };
    let set_cookie_str = Cookie::build((COOKIE_NAME.to_string(), signed_value))
        .path("/")
        .secure(config.secure)
        .http_only(config.http_only)
        .same_site(same_site)
        .max_age(cookie::time::Duration::seconds(max_age_secs))
        .build()
        .to_string();

    match HeaderValue::from_str(&set_cookie_str) {
        Ok(v) => {
            response.headers_mut().append(http::header::SET_COOKIE, v);
        }
        Err(e) => {
            tracing::error!("failed to set flash cookie header: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::routing::get;
    use http::StatusCode;
    use tower::ServiceExt;

    fn test_config() -> CookieConfig {
        CookieConfig {
            secret: "a".repeat(64),
            secure: false,
            http_only: true,
            same_site: "lax".into(),
        }
    }

    fn test_key(config: &CookieConfig) -> Key {
        crate::cookie::key_from_config(config).unwrap()
    }

    fn make_signed_cookie(entries: &[FlashEntry], key: &Key) -> String {
        let json = serde_json::to_string(entries).unwrap();
        let mut jar = CookieJar::new();
        jar.signed_mut(key)
            .add(Cookie::new(COOKIE_NAME.to_string(), json));
        let signed_value = jar.get(COOKIE_NAME).unwrap().value().to_string();
        format!("{COOKIE_NAME}={signed_value}")
    }

    /// Extract the flash Set-Cookie header from response (handles multiple Set-Cookie headers)
    fn extract_flash_set_cookie(resp: &Response<Body>) -> Option<String> {
        resp.headers()
            .get_all(http::header::SET_COOKIE)
            .iter()
            .find_map(|v| {
                let s = v.to_str().ok()?;
                if s.starts_with("flash=") {
                    Some(s.to_string())
                } else {
                    None
                }
            })
    }

    // --- Module-level handlers (axum Handler bounds require module-level async fn) ---

    async fn noop_handler() -> StatusCode {
        StatusCode::OK
    }

    async fn set_flash_handler(flash: crate::flash::Flash) -> StatusCode {
        flash.success("it worked");
        StatusCode::OK
    }

    async fn set_multiple_handler(flash: crate::flash::Flash) -> StatusCode {
        flash.error("bad");
        flash.warning("careful");
        StatusCode::OK
    }

    async fn mark_read_handler(req: Request<Body>) -> StatusCode {
        let state = req.extensions().get::<Arc<FlashState>>().unwrap();
        state.mark_read();
        StatusCode::OK
    }

    async fn read_and_write_handler(req: Request<Body>) -> StatusCode {
        let state = req.extensions().get::<Arc<FlashState>>().unwrap();
        state.mark_read();
        state.push("success", "new");
        StatusCode::OK
    }

    // --- Tests ---

    #[tokio::test]
    async fn no_cookie_empty_state_no_set_cookie() {
        let config = test_config();
        let key = test_key(&config);
        let app = Router::new()
            .route("/", get(noop_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(extract_flash_set_cookie(&resp).is_none());
    }

    #[tokio::test]
    async fn outgoing_writes_cookie() {
        let config = test_config();
        let key = test_key(&config);
        let app = Router::new()
            .route("/", get(set_flash_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let cookie_str = extract_flash_set_cookie(&resp).expect("should have Set-Cookie");
        assert!(cookie_str.contains("flash="));
        assert!(cookie_str.contains("HttpOnly"));
    }

    #[tokio::test]
    async fn valid_signed_cookie_populates_incoming() {
        let config = test_config();
        let key = test_key(&config);

        let entries = vec![FlashEntry {
            level: "success".into(),
            message: "saved".into(),
        }];
        let cookie_val = make_signed_cookie(&entries, &key);

        let app = Router::new()
            .route("/", get(noop_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder()
            .uri("/")
            .header(http::header::COOKIE, cookie_val)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // No read, no write — cookie untouched
        assert!(extract_flash_set_cookie(&resp).is_none());
    }

    #[tokio::test]
    async fn invalid_cookie_gives_empty_incoming() {
        let config = test_config();
        let key = test_key(&config);

        let app = Router::new()
            .route("/", get(noop_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder()
            .uri("/")
            .header(http::header::COOKIE, "flash=tampered_value")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(extract_flash_set_cookie(&resp).is_none());
    }

    #[tokio::test]
    async fn read_flag_clears_cookie() {
        let config = test_config();
        let key = test_key(&config);

        let entries = vec![FlashEntry {
            level: "info".into(),
            message: "hello".into(),
        }];
        let cookie_val = make_signed_cookie(&entries, &key);

        let app = Router::new()
            .route("/", get(mark_read_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder()
            .uri("/")
            .header(http::header::COOKIE, cookie_val)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        let cookie_str = extract_flash_set_cookie(&resp).expect("should clear cookie");
        assert!(cookie_str.contains("Max-Age=0"));
    }

    #[tokio::test]
    async fn outgoing_plus_read_writes_only_outgoing() {
        let config = test_config();
        let key = test_key(&config);

        let entries = vec![FlashEntry {
            level: "info".into(),
            message: "old".into(),
        }];
        let cookie_val = make_signed_cookie(&entries, &key);

        let app = Router::new()
            .route("/", get(read_and_write_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder()
            .uri("/")
            .header(http::header::COOKIE, cookie_val)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        let cookie_str = extract_flash_set_cookie(&resp).expect("should have cookie");
        assert!(!cookie_str.contains("Max-Age=0"));
        assert!(cookie_str.contains("flash="));
    }

    #[tokio::test]
    async fn multiple_outgoing_messages() {
        let config = test_config();
        let key = test_key(&config);
        let app = Router::new()
            .route("/", get(set_multiple_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();

        let cookie_str = extract_flash_set_cookie(&resp).expect("should have Set-Cookie");
        assert!(cookie_str.contains("flash="));
    }

    #[tokio::test]
    async fn cookie_attributes_applied() {
        let config = CookieConfig {
            secret: "a".repeat(64),
            secure: true,
            http_only: true,
            same_site: "strict".into(),
        };
        let key = test_key(&config);
        let app = Router::new()
            .route("/", get(set_flash_handler))
            .layer(FlashLayer::new(&config, &key));

        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();

        let cookie_str = extract_flash_set_cookie(&resp).expect("should have Set-Cookie");
        assert!(cookie_str.contains("Secure"));
        assert!(cookie_str.contains("HttpOnly"));
        assert!(cookie_str.contains("SameSite=Strict"));
        assert!(cookie_str.contains("Path=/"));
    }
}
