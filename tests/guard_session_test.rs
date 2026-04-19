//! Integration tests for session-based guards wired with real `CookieSessionLayer`.

use axum::Router;
use axum::body::Body;
use axum::routing::get;
use http::{Request, StatusCode};
use modo::guards;
use modo::service::Registry;
use modo::testing::{TestDb, TestSession};
use tower::ServiceExt;

async fn ok_handler() -> &'static str {
    "ok"
}

// --- require_authenticated ---

#[tokio::test]
async fn require_authenticated_redirects_anonymous_request() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = Router::new()
        .route("/app", get(ok_handler))
        .route_layer(guards::require_authenticated("/auth"))
        .layer(session.layer())
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(Request::get("/app").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get("location").unwrap(), "/auth");
}

#[tokio::test]
async fn require_authenticated_passes_with_valid_session_cookie() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;
    let cookie = session.authenticate("user-1").await;

    let app = Router::new()
        .route("/app", get(ok_handler))
        .route_layer(guards::require_authenticated("/auth"))
        .layer(session.layer())
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(
            Request::get("/app")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn require_authenticated_htmx_anonymous_returns_hx_redirect() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = Router::new()
        .route("/app", get(ok_handler))
        .route_layer(guards::require_authenticated("/auth"))
        .layer(session.layer())
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(
            Request::get("/app")
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("hx-redirect").unwrap(), "/auth");
}

// --- require_unauthenticated ---

#[tokio::test]
async fn require_unauthenticated_passes_for_anonymous_request() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = Router::new()
        .route("/auth", get(ok_handler))
        .route_layer(guards::require_unauthenticated("/app"))
        .layer(session.layer())
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(Request::get("/auth").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn require_unauthenticated_redirects_signed_in_caller() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;
    let cookie = session.authenticate("user-1").await;

    let app = Router::new()
        .route("/auth", get(ok_handler))
        .route_layer(guards::require_unauthenticated("/app"))
        .layer(session.layer())
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(
            Request::get("/auth")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get("location").unwrap(), "/app");
}

#[tokio::test]
async fn require_unauthenticated_htmx_signed_in_returns_hx_redirect() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;
    let cookie = session.authenticate("user-1").await;

    let app = Router::new()
        .route("/auth", get(ok_handler))
        .route_layer(guards::require_unauthenticated("/app"))
        .layer(session.layer())
        .with_state(Registry::new().into_state());

    let resp = app
        .oneshot(
            Request::get("/auth")
                .header("cookie", cookie)
                .header("hx-request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("hx-redirect").unwrap(), "/app");
}
