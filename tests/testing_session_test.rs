#![cfg(feature = "test-helpers")]

use axum::routing::get;
use modo::session::Session;
use modo::testing::{TestApp, TestDb, TestSession};

async fn whoami(session: Session) -> String {
    match session.user_id() {
        Some(uid) => uid,
        None => "anonymous".to_string(),
    }
}

async fn session_data(session: Session) -> String {
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
