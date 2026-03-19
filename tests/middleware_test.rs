use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use http::StatusCode;
use modo::service::Registry;
use tower::ServiceExt;

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
        .layer(modo::middleware::security_headers(&config))
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

    let config = modo::middleware::SecurityHeadersConfig {
        hsts_max_age: Some(31536000),
        ..Default::default()
    };
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::security_headers(&config))
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

    let config = modo::middleware::SecurityHeadersConfig {
        content_security_policy: Some("default-src 'self'".to_string()),
        permissions_policy: Some("geolocation=()".to_string()),
        ..Default::default()
    };
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::security_headers(&config))
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

    let config = modo::middleware::SecurityHeadersConfig {
        x_content_type_options: false,
        ..Default::default()
    };
    let app = Router::new()
        .route("/", get(handler))
        .layer(modo::middleware::security_headers(&config))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert!(response.headers().get("x-content-type-options").is_none());
}
