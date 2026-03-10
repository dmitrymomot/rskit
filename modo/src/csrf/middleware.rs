use super::config::CsrfConfig;
use super::token;
use crate::cookie_util::read_cookie;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use http::header;

/// CSRF token inserted into request extensions by the middleware.
///
/// Handlers can extract this to get the raw token value.
#[derive(Debug, Clone)]
pub struct CsrfToken(pub String);

/// Double-submit cookie CSRF protection middleware.
///
/// For safe methods (GET, HEAD, OPTIONS, TRACE): generates a token, sets it
/// in a signed cookie, and injects it into request extensions.
///
/// For mutating methods (POST, PUT, PATCH, DELETE): validates that the token
/// submitted via header or form field matches the cookie token.
pub async fn csrf_protection(
    State(state): State<crate::app::AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let arc_config = state.services.get::<CsrfConfig>();
    let default_config;
    let config: &CsrfConfig = match &arc_config {
        Some(c) => c,
        None => {
            default_config = CsrfConfig::default();
            &default_config
        }
    };

    debug_assert!(
        config.validate().is_ok(),
        "Invalid CsrfConfig: {:?}",
        config.validate()
    );

    let key = state.server_config.secret_key.as_bytes();
    let method = request.method().clone();

    if is_safe_method(&method) {
        handle_safe_request(request, next, config, key).await
    } else {
        handle_mutating_request(request, next, config, key).await
    }
}

fn is_safe_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
    )
}

async fn handle_safe_request(
    request: Request<Body>,
    next: Next,
    config: &CsrfConfig,
    key: &[u8],
) -> Response {
    let (mut parts, body) = request.into_parts();

    // Try to read existing valid token from cookie
    let existing_token = read_cookie(&parts.headers, &config.cookie_name)
        .and_then(|signed| token::verify(&signed, key));

    let (raw_token, needs_cookie) = match existing_token {
        Some(t) => (t, false),
        None => (token::generate(config.token_length), true),
    };

    // Insert token into extensions for handlers
    parts.extensions.insert(CsrfToken(raw_token.clone()));

    // Insert into TemplateContext if available
    #[cfg(feature = "templates")]
    if let Some(ctx) = parts
        .extensions
        .get_mut::<crate::templates::TemplateContext>()
    {
        ctx.insert("csrf_token", raw_token.clone());
        ctx.insert("csrf_field_name", config.field_name.clone());
    }

    let request = Request::from_parts(parts, body);
    let mut response = next.run(request).await;

    if needs_cookie {
        let signed = token::sign(&raw_token, key);
        let cookie = build_set_cookie(
            &config.cookie_name,
            &signed,
            config.cookie_max_age,
            config.secure,
        );
        if let Ok(val) = cookie.parse() {
            response.headers_mut().append(header::SET_COOKIE, val);
        }
    }

    response
}

async fn handle_mutating_request(
    request: Request<Body>,
    next: Next,
    config: &CsrfConfig,
    key: &[u8],
) -> Response {
    let (mut parts, body) = request.into_parts();

    // 1. Read and verify cookie token
    let cookie_token = match read_cookie(&parts.headers, &config.cookie_name)
        .and_then(|signed| token::verify(&signed, key))
    {
        Some(t) => t,
        None => {
            tracing::warn!("CSRF validation failed: missing or invalid cookie");
            return StatusCode::FORBIDDEN.into_response();
        }
    };

    // 2. Extract submitted token from header first
    let submitted = parts
        .headers
        .get(&config.header_name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // 3. If no header, try form body (only for url-encoded)
    let (submitted, body) = if submitted.is_some() {
        (submitted, body)
    } else {
        extract_from_form_body(
            &parts.headers,
            body,
            &config.field_name,
            config.max_body_bytes,
        )
        .await
    };

    let submitted = match submitted {
        Some(t) => t,
        None => {
            tracing::warn!("CSRF validation failed: no token in header or form body");
            return StatusCode::FORBIDDEN.into_response();
        }
    };

    // 4. Constant-time compare
    if !constant_time_eq(cookie_token.as_bytes(), submitted.as_bytes()) {
        tracing::warn!("CSRF validation failed: token mismatch");
        return StatusCode::FORBIDDEN.into_response();
    }

    // 5. Inject token so handlers re-rendering forms (e.g. validation errors) can access it
    parts.extensions.insert(CsrfToken(cookie_token.clone()));

    #[cfg(feature = "templates")]
    if let Some(ctx) = parts
        .extensions
        .get_mut::<crate::templates::TemplateContext>()
    {
        ctx.insert("csrf_token", cookie_token.clone());
        ctx.insert("csrf_field_name", config.field_name.clone());
    }

    let request = Request::from_parts(parts, body);
    next.run(request).await
}

/// Extract the CSRF token from a url-encoded form body, returning both the
/// token (if found) and the reconstructed body for downstream handlers.
async fn extract_from_form_body(
    headers: &http::HeaderMap,
    body: Body,
    field_name: &str,
    max_body_bytes: usize,
) -> (Option<String>, Body) {
    let is_form = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("application/x-www-form-urlencoded"));

    if !is_form {
        return (None, body);
    }

    // Buffer the body
    let bytes = match axum::body::to_bytes(body, max_body_bytes).await {
        Ok(b) => b,
        Err(_) => return (None, Body::empty()),
    };

    // Parse url-encoded form
    let token = form_urlencoded::parse(&bytes)
        .find(|(key, _)| key == field_name)
        .map(|(_, val)| val.into_owned());

    // Reconstruct body for downstream
    (token, Body::from(bytes))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}

// --- Cookie helpers ---

fn build_set_cookie(name: &str, value: &str, max_age: u64, secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    format!("{name}={value}; HttpOnly; SameSite=Lax; Path=/; Max-Age={max_age}{secure_flag}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppState, ServiceRegistry};
    use crate::config::ServerConfig;
    use axum::Router;
    use axum::routing::{get, post};
    use axum_extra::extract::cookie::Key;
    use http::Request;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        let services = ServiceRegistry::new().with(CsrfConfig::default());

        let server_config = ServerConfig {
            secret_key: "test-secret-key-for-csrf".to_string(),
            ..Default::default()
        };

        AppState {
            services,
            server_config,
            cookie_key: Key::generate(),
        }
    }

    fn test_app(state: AppState) -> Router {
        Router::new()
            .route(
                "/form",
                get(|_req: axum::http::Request<Body>| async { "ok" }),
            )
            .route(
                "/submit",
                post(|_req: axum::http::Request<Body>| async { "ok" }),
            )
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                csrf_protection,
            ))
            .with_state(state)
    }

    fn extract_set_cookie(response: &Response) -> Option<String> {
        response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }

    #[tokio::test]
    async fn get_with_no_cookie_sets_cookie_and_injects_token() {
        let app = test_app(test_state());
        let request = Request::builder().uri("/form").body(Body::empty()).unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let set_cookie = extract_set_cookie(&response).expect("should set cookie");
        assert!(set_cookie.contains("_csrf="));
        assert!(set_cookie.contains("HttpOnly"));
        assert!(set_cookie.contains("SameSite=Lax"));
        assert!(set_cookie.contains("Path=/"));
    }

    #[tokio::test]
    async fn get_with_valid_cookie_does_not_set_new_cookie() {
        let state = test_state();
        let config = state
            .services
            .get::<CsrfConfig>()
            .map(|c| (*c).clone())
            .unwrap_or_default();
        let key = state.server_config.secret_key.as_bytes();
        let raw_token = token::generate(config.token_length);
        let signed = token::sign(&raw_token, key);

        let app = test_app(state);
        let request = Request::builder()
            .uri("/form")
            .header(header::COOKIE, format!("_csrf={signed}"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(extract_set_cookie(&response).is_none());
    }

    #[tokio::test]
    async fn post_with_valid_cookie_and_matching_header_succeeds() {
        let state = test_state();
        let key = state.server_config.secret_key.as_bytes();
        let raw_token = token::generate(32);
        let signed = token::sign(&raw_token, key);

        let app = test_app(state);
        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header(header::COOKIE, format!("_csrf={signed}"))
            .header("x-csrf-token", &raw_token)
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn post_with_valid_cookie_and_matching_form_field_succeeds() {
        let state = test_state();
        let key = state.server_config.secret_key.as_bytes();
        let raw_token = token::generate(32);
        let signed = token::sign(&raw_token, key);
        let form_body = format!("name=test&_csrf_token={raw_token}");

        let app = test_app(state);
        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header(header::COOKIE, format!("_csrf={signed}"))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(form_body))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn post_with_wrong_token_returns_403() {
        let state = test_state();
        let key = state.server_config.secret_key.as_bytes();
        let raw_token = token::generate(32);
        let signed = token::sign(&raw_token, key);

        let app = test_app(state);
        let wrong_token = token::generate(32);
        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header(header::COOKIE, format!("_csrf={signed}"))
            .header("x-csrf-token", &wrong_token)
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn post_with_no_cookie_returns_403() {
        let app = test_app(test_state());
        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header("x-csrf-token", "some-token")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn post_with_tampered_cookie_hmac_returns_403() {
        let state = test_state();
        let key = state.server_config.secret_key.as_bytes();
        let raw_token = token::generate(32);
        let mut signed = token::sign(&raw_token, key);
        // Tamper the last char
        let last = signed.pop().unwrap();
        signed.push(if last == 'a' { 'b' } else { 'a' });

        let app = test_app(state);
        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header(header::COOKIE, format!("_csrf={signed}"))
            .header("x-csrf-token", &raw_token)
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn post_with_no_submitted_token_returns_403() {
        let state = test_state();
        let key = state.server_config.secret_key.as_bytes();
        let raw_token = token::generate(32);
        let signed = token::sign(&raw_token, key);

        let app = test_app(state);
        let request = Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header(header::COOKIE, format!("_csrf={signed}"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn head_skips_validation() {
        let app = test_app(test_state());
        let request = Request::builder()
            .method(Method::HEAD)
            .uri("/form")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn build_set_cookie_format() {
        let cookie = build_set_cookie("_csrf", "token123", 86400, false);
        assert_eq!(
            cookie,
            "_csrf=token123; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400"
        );
    }

    #[test]
    fn build_set_cookie_secure() {
        let cookie = build_set_cookie("_csrf", "token123", 86400, true);
        assert!(cookie.contains("; Secure"));
    }

    #[test]
    fn read_cookie_finds_value() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "other=x; _csrf=mytoken; foo=bar".parse().unwrap(),
        );
        assert_eq!(read_cookie(&headers, "_csrf").unwrap(), "mytoken");
    }

    #[test]
    fn read_cookie_returns_none_when_missing() {
        let mut headers = http::HeaderMap::new();
        headers.insert(header::COOKIE, "other=x".parse().unwrap());
        assert!(read_cookie(&headers, "_csrf").is_none());
    }
}
