use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use http::StatusCode;
use modo::middleware::CsrfConfig;
use modo::service::Registry;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

use modo::middleware::GlobalKeyExtractor;

// ---------------------------------------------------------------------------
// CORS
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cors_allows_configured_origin() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = {
        let mut c = modo::middleware::CorsConfig::default();
        c.origins = vec!["https://example.com".to_string()];
        c
    };
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::cors(&config))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("origin", "https://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let allow_origin = response.headers().get("access-control-allow-origin");
    assert!(allow_origin.is_some());
    assert_eq!(
        allow_origin.unwrap().to_str().unwrap(),
        "https://example.com"
    );
}

#[tokio::test]
async fn test_cors_default_allows_any_origin_no_credentials() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = modo::middleware::CorsConfig::default();
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::cors(&config))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("origin", "https://anything.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let allow_origin = response.headers().get("access-control-allow-origin");
    assert!(allow_origin.is_some());
    assert_eq!(allow_origin.unwrap().to_str().unwrap(), "*");
}

#[tokio::test]
async fn test_cors_rejects_unlisted_origin() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = {
        let mut c = modo::middleware::CorsConfig::default();
        c.origins = vec!["https://example.com".to_string()];
        c
    };
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::cors(&config))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("origin", "https://evil.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // The request still succeeds, but no allow-origin header is set
    assert_eq!(response.status(), StatusCode::OK);
    let allow_origin = response.headers().get("access-control-allow-origin");
    assert!(allow_origin.is_none());
}

#[tokio::test]
async fn test_cors_with_predicate() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = {
        let mut c = modo::middleware::CorsConfig::default();
        c.origins = vec!["https://example.com".to_string()];
        c
    };
    let predicate = modo::middleware::urls(&config.origins);
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::cors_with(&config, predicate))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("origin", "https://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let allow_origin = response.headers().get("access-control-allow-origin");
    assert!(allow_origin.is_some());
}

#[tokio::test]
async fn test_cors_subdomains_predicate() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = modo::middleware::CorsConfig::default();
    let predicate = modo::middleware::subdomains("example.com");
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::cors_with(&config, predicate))
        .with_state(Registry::new().into_state());

    // Subdomain should be allowed
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("origin", "https://api.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_some()
    );

    // Exact domain should also be allowed
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("origin", "https://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_some()
    );

    // Unrelated domain should not be allowed
    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("origin", "https://evil.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
}

#[tokio::test]
async fn test_request_id_sets_header() {
    async fn handler() -> &'static str {
        "ok"
    }

    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::request_id())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let request_id = response.headers().get("x-request-id");
    assert!(request_id.is_some());
    assert_eq!(request_id.unwrap().len(), 26); // ULID length
}

#[tokio::test]
async fn test_request_id_preserves_existing() {
    async fn handler() -> &'static str {
        "ok"
    }

    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::request_id())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("x-request-id", "existing-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let request_id = response.headers().get("x-request-id");
    assert!(request_id.is_some());
    assert_eq!(request_id.unwrap().to_str().unwrap(), "existing-id");
}

#[tokio::test]
async fn test_compression_layer_compiles() {
    async fn handler() -> &'static str {
        "ok"
    }

    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::compression())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_tracing_layer_compiles() {
    async fn handler() -> &'static str {
        "ok"
    }

    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::tracing())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_catch_panic_returns_500() {
    async fn panicking_handler() -> &'static str {
        panic!("boom");
    }

    let app = Router::new()
        .route("/", get(panicking_handler))
        .layer(modo::middleware::catch_panic())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // Verify modo::Error is in response extensions
    let error = response.extensions().get::<modo::Error>();
    assert!(error.is_some());
}

#[tokio::test]
async fn test_security_headers_defaults() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = modo::middleware::SecurityHeadersConfig::default();
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::security_headers(&config).unwrap())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(
        response.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert_eq!(response.headers().get("x-frame-options").unwrap(), "DENY");
    assert_eq!(
        response.headers().get("referrer-policy").unwrap(),
        "strict-origin-when-cross-origin"
    );
}

#[tokio::test]
async fn test_security_headers_hsts() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = {
        let mut c = modo::middleware::SecurityHeadersConfig::default();
        c.hsts_max_age = Some(31536000);
        c
    };
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::security_headers(&config).unwrap())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(
        response.headers().get("strict-transport-security").unwrap(),
        "max-age=31536000"
    );
}

#[tokio::test]
async fn test_security_headers_csp_and_permissions() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = {
        let mut c = modo::middleware::SecurityHeadersConfig::default();
        c.content_security_policy = Some("default-src 'self'".to_string());
        c.permissions_policy = Some("geolocation=()".to_string());
        c
    };
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::security_headers(&config).unwrap())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(
        response.headers().get("content-security-policy").unwrap(),
        "default-src 'self'"
    );
    assert_eq!(
        response.headers().get("permissions-policy").unwrap(),
        "geolocation=()"
    );
}

#[tokio::test]
async fn test_security_headers_disabled_x_content_type_options() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = {
        let mut c = modo::middleware::SecurityHeadersConfig::default();
        c.x_content_type_options = false;
        c
    };
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::security_headers(&config).unwrap())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert!(response.headers().get("x-content-type-options").is_none());
}

// ---------------------------------------------------------------------------
// CSRF
// ---------------------------------------------------------------------------

fn test_csrf_config() -> CsrfConfig {
    CsrfConfig::default()
}

fn test_cookie_key() -> modo::cookie::Key {
    modo::cookie::Key::from(&[0u8; 64])
}

#[tokio::test]
async fn test_csrf_get_sets_cookie() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = test_csrf_config();
    let key = test_cookie_key();
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::csrf(&config, &key))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response.headers().get("set-cookie");
    assert!(set_cookie.is_some(), "GET should set CSRF cookie");
    let cookie_str = set_cookie.unwrap().to_str().unwrap();
    assert!(cookie_str.contains("_csrf"), "cookie name should be _csrf");
}

#[tokio::test]
async fn test_csrf_rejects_post_without_token() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = test_csrf_config();
    let key = test_cookie_key();
    let app = Router::new()
        .route("/", axum::routing::post(handler))
        .layer(modo::middleware::csrf(&config, &key))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_csrf_accepts_post_with_valid_header_token() {
    // Full round-trip test:
    // 1. GET to obtain CSRF cookie
    // 2. POST with cookie + X-CSRF-Token header

    async fn handler() -> &'static str {
        "ok"
    }

    let config = test_csrf_config();
    let key = test_cookie_key();

    let app = Router::new()
        .route("/", get(handler))
        .route("/submit", axum::routing::post(handler))
        .layer(modo::middleware::csrf(&config, &key))
        .with_state(Registry::new().into_state());

    // Step 1: GET to obtain the CSRF cookie
    let get_response = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let set_cookie = get_response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();
    // Extract the cookie name=value pair (before the first ';')
    let cookie_value = set_cookie.split(';').next().unwrap(); // e.g., "_csrf=SIGNED_TOKEN"

    // The CsrfToken should be in response extensions
    let csrf_token = get_response
        .extensions()
        .get::<modo::middleware::CsrfToken>();
    assert!(
        csrf_token.is_some(),
        "CsrfToken should be in response extensions"
    );
    let token_value = csrf_token.unwrap().0.clone();

    // Step 2: POST with cookie and header
    let post_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/submit")
                .header("cookie", cookie_value)
                .header("x-csrf-token", &token_value)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(post_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_csrf_skips_exempt_methods() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = test_csrf_config();
    let key = test_cookie_key();
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::csrf(&config, &key))
        .with_state(Registry::new().into_state());

    // HEAD should be exempt
    let response = app
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Rate Limiting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_rate_limit_config_defaults() {
    let config = modo::middleware::RateLimitConfig::default();
    assert_eq!(config.per_second, 1);
    assert_eq!(config.burst_size, 10);
    assert!(config.use_headers);
    assert_eq!(config.cleanup_interval_secs, 60);
    assert_eq!(config.max_keys, 10_000);
}

#[tokio::test]
async fn test_rate_limit_config_deserialize() {
    let yaml = r#"
per_second: 5
burst_size: 20
use_headers: false
cleanup_interval_secs: 120
max_keys: 5000
"#;
    let config: modo::middleware::RateLimitConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.per_second, 5);
    assert_eq!(config.burst_size, 20);
    assert!(!config.use_headers);
    assert_eq!(config.cleanup_interval_secs, 120);
    assert_eq!(config.max_keys, 5000);
}

#[tokio::test]
async fn test_rate_limit_config_deserialize_partial() {
    let yaml = r#"
per_second: 3
"#;
    let config: modo::middleware::RateLimitConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.per_second, 3);
    // Remaining fields should use defaults
    assert_eq!(config.burst_size, 10);
    assert!(config.use_headers);
    assert_eq!(config.cleanup_interval_secs, 60);
    assert_eq!(config.max_keys, 10_000);
}

#[tokio::test]
async fn test_rate_limit_allows_within_burst() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = {
        let mut c = modo::middleware::RateLimitConfig::default();
        c.per_second = 1;
        c.burst_size = 5;
        c.use_headers = true;
        c.cleanup_interval_secs = 60;
        c.max_keys = 10_000;
        c
    };
    // Use GlobalKeyExtractor because oneshot tests lack ConnectInfo<SocketAddr>
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::rate_limit_with(
            &config,
            GlobalKeyExtractor,
            CancellationToken::new(),
        ))
        .with_state(Registry::new().into_state());

    // First request should succeed (within burst of 5)
    let response = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_rate_limit_includes_headers() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = {
        let mut c = modo::middleware::RateLimitConfig::default();
        c.per_second = 1;
        c.burst_size = 5;
        c.use_headers = true;
        c.cleanup_interval_secs = 60;
        c.max_keys = 10_000;
        c
    };
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::rate_limit_with(
            &config,
            GlobalKeyExtractor,
            CancellationToken::new(),
        ))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    // When use_headers is true, rate limit headers should be present
    assert!(
        response.headers().get("x-ratelimit-limit").is_some(),
        "expected x-ratelimit-limit header"
    );
    assert!(
        response.headers().get("x-ratelimit-remaining").is_some(),
        "expected x-ratelimit-remaining header"
    );
}

#[tokio::test]
async fn test_rate_limit_rejects_over_burst() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = {
        let mut c = modo::middleware::RateLimitConfig::default();
        c.per_second = 1;
        c.burst_size = 2;
        c.use_headers = true;
        c.cleanup_interval_secs = 60;
        c.max_keys = 10_000;
        c
    };
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::rate_limit_with(
            &config,
            GlobalKeyExtractor,
            CancellationToken::new(),
        ))
        .with_state(Registry::new().into_state());

    // Exhaust the burst (2 requests)
    for _ in 0..2 {
        let resp = app
            .clone()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // Third request should be rate-limited
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    // modo::Error should be in extensions
    let error = response.extensions().get::<modo::Error>();
    assert!(error.is_some(), "expected modo::Error in extensions");
    assert_eq!(error.unwrap().status(), StatusCode::TOO_MANY_REQUESTS);
}

// ---------------------------------------------------------------------------
// Error Handler
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_error_handler_rewrites_handler_errors() {
    async fn failing_handler() -> modo::Result<String> {
        Err(modo::Error::not_found("not here"))
    }

    async fn my_error_handler(
        err: modo::Error,
        _parts: http::request::Parts,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        (err.status(), format!("custom: {}", err.message())).into_response()
    }

    let app = Router::new()
        .route("/", get(failing_handler))
        .layer(modo::middleware::error_handler(my_error_handler))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("custom: not here"));
}

#[tokio::test]
async fn test_error_handler_passes_through_success() {
    async fn ok_handler() -> &'static str {
        "ok"
    }

    async fn my_error_handler(
        _err: modo::Error,
        _parts: http::request::Parts,
    ) -> axum::response::Response {
        unreachable!("should not be called for 200");
    }

    let app = Router::new()
        .route("/", get(ok_handler))
        .layer(modo::middleware::error_handler(my_error_handler))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_error_handler_catches_panic_errors() {
    async fn panicking() -> &'static str {
        panic!("boom")
    }

    async fn my_error_handler(
        err: modo::Error,
        _parts: http::request::Parts,
    ) -> axum::response::Response {
        use axum::response::IntoResponse;
        (err.status(), format!("caught: {}", err.message())).into_response()
    }

    let app = Router::new()
        .route("/", get(panicking))
        .layer(modo::middleware::catch_panic())
        .layer(modo::middleware::error_handler(my_error_handler))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("caught:"));
}

#[tokio::test]
async fn test_security_headers_does_not_overwrite_handler_set() {
    async fn handler_with_custom_frame_options() -> axum::response::Response {
        use axum::response::IntoResponse;
        let mut resp = "ok".into_response();
        resp.headers_mut().insert(
            http::HeaderName::from_static("x-frame-options"),
            http::HeaderValue::from_static("SAMEORIGIN"),
        );
        resp
    }

    let config = modo::middleware::SecurityHeadersConfig::default();
    let app = Router::new()
        .route("/", get(handler_with_custom_frame_options))
        .layer(modo::middleware::security_headers(&config).unwrap())
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    // Handler's SAMEORIGIN should be preserved, not overwritten by config's DENY
    assert_eq!(
        response.headers().get("x-frame-options").unwrap(),
        "SAMEORIGIN"
    );
    // Other security headers should still be set
    assert_eq!(
        response.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
}

// ---------------------------------------------------------------------------
// Task 8: Additional CORS and rate-limit tests
// ---------------------------------------------------------------------------

/// Verify that an OPTIONS preflight request with a matching `Origin` and
/// `Access-Control-Request-Method` header receives the appropriate CORS
/// allow headers in the response.
#[tokio::test]
async fn test_cors_preflight_options() {
    async fn handler() -> &'static str {
        "ok"
    }

    let config = {
        let mut c = modo::middleware::CorsConfig::default();
        c.origins = vec!["https://example.com".to_string()];
        c
    };
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::cors(&config))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/")
                .header("origin", "https://example.com")
                .header("access-control-request-method", "POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Preflight responses should be 200 (tower-http CorsLayer returns 200 for
    // valid preflight requests when the origin is allowed).
    assert!(
        response.status().is_success(),
        "preflight should succeed, got {}",
        response.status()
    );

    let allow_origin = response.headers().get("access-control-allow-origin");
    assert!(
        allow_origin.is_some(),
        "expected Access-Control-Allow-Origin header in preflight response"
    );
    assert_eq!(
        allow_origin.unwrap().to_str().unwrap(),
        "https://example.com"
    );

    let allow_methods = response.headers().get("access-control-allow-methods");
    assert!(
        allow_methods.is_some(),
        "expected Access-Control-Allow-Methods header in preflight response"
    );
}

// NOTE: `test_rate_limit_default_extractor` is intentionally omitted.
//
// The default `rate_limit()` function uses `PeerIpKeyExtractor`, which reads
// the peer IP from `ConnectInfo<SocketAddr>` in the request extensions. That
// extension is only present when the server is started with
// `into_make_service_with_connect_info::<SocketAddr>()`. The `tower::ServiceExt::oneshot()`
// helper used in these unit-style integration tests does not inject
// `ConnectInfo`, so any request would cause `PeerIpKeyExtractor::extract` to
// return `None`, resulting in a 500 "unable to extract rate-limit key" response
// rather than exercising the rate-limit logic. Use `rate_limit_with` together
// with `GlobalKeyExtractor` (as the existing `test_rate_limit_*` tests above
// already demonstrate) to test rate-limit behaviour without a real TCP listener.
