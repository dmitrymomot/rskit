#![cfg(feature = "test-helpers")]

use axum::routing::get;
use modo::auth::session::{CookieSession, Session, SessionConfig};
use modo::cookie::CookieConfig;
use modo::testing::{TestApp, TestDb, TestSession};

/// Read the authenticated user's ID.
/// Uses the data extractor `Session` (transport-agnostic snapshot).
async fn whoami(session: Option<Session>) -> String {
    match session {
        Some(s) => s.user_id,
        None => "anonymous".to_string(),
    }
}

/// Read a JSON key from session data.
/// Uses `CookieSession` for the .get() helper.
async fn session_data(session: CookieSession) -> String {
    let role: Option<String> = session.get("role").unwrap_or(None);
    role.unwrap_or_else(|| "none".to_string())
}

#[tokio::test]
async fn test_unauthenticated_request() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = TestApp::builder()
        .route("/me", get(whoami))
        .layer(session.layer())
        .build();

    let res = app.get("/me").send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "anonymous");
}

#[tokio::test]
async fn test_authenticated_request() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = TestApp::builder()
        .route("/me", get(whoami))
        .layer(session.layer())
        .build();

    let cookie = session.authenticate("user-42").await;

    let res = app.get("/me").header("cookie", &cookie).send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "user-42");
}

#[tokio::test]
async fn test_multiple_users() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = TestApp::builder()
        .route("/me", get(whoami))
        .layer(session.layer())
        .build();

    let cookie_a = session.authenticate("alice").await;
    let cookie_b = session.authenticate("bob").await;

    let res_a = app.get("/me").header("cookie", &cookie_a).send().await;
    let res_b = app.get("/me").header("cookie", &cookie_b).send().await;

    assert_eq!(res_a.text(), "alice");
    assert_eq!(res_b.text(), "bob");
}

#[tokio::test]
async fn test_authenticate_with_custom_data() {
    let db = TestDb::new().await;
    let session = TestSession::new(&db).await;

    let app = TestApp::builder()
        .route("/role", get(session_data))
        .layer(session.layer())
        .build();

    let cookie = session
        .authenticate_with("user-1", serde_json::json!({"role": "admin"}))
        .await;

    let res = app.get("/role").header("cookie", &cookie).send().await;
    assert_eq!(res.text(), "admin");
}

#[tokio::test]
async fn test_with_custom_cookie_name() {
    let db = TestDb::new().await;

    let session_config = {
        let mut c = SessionConfig::default();
        c.cookie_name = "my_sess".to_string();
        c
    };
    let cookie_config = {
        let mut c = CookieConfig::new("b".repeat(64));
        c.secure = false;
        c
    };

    let session = TestSession::with_config(&db, session_config, cookie_config).await;
    let cookie = session.authenticate("user-99").await;

    assert!(
        cookie.starts_with("my_sess="),
        "expected cookie to start with 'my_sess=', got: {cookie}"
    );
}

#[tokio::test]
async fn test_with_custom_config_still_authenticates() {
    let db = TestDb::new().await;

    let session_config = {
        let mut c = SessionConfig::default();
        c.cookie_name = "custom_session".to_string();
        c.session_ttl_secs = 60;
        c.validate_fingerprint = false;
        c
    };
    let cookie_config = {
        let mut c = CookieConfig::new("c".repeat(64));
        c.secure = false;
        c.same_site = "strict".to_string();
        c
    };

    let session = TestSession::with_config(&db, session_config, cookie_config).await;

    let app = TestApp::builder()
        .route("/me", get(whoami))
        .layer(session.layer())
        .build();

    let cookie = session.authenticate("custom-user").await;

    let res = app.get("/me").header("cookie", &cookie).send().await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "custom-user");
}
