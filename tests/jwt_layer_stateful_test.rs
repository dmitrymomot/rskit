#![cfg(feature = "test-helpers")]

use axum::routing::get;
use modo::auth::session::Session;
use modo::auth::session::jwt::{JwtSessionService, JwtSessionsConfig};
use modo::auth::session::meta::SessionMeta;
use modo::db::ConnExt;
use modo::testing::{TestApp, TestDb, TestSession};

fn meta() -> SessionMeta {
    SessionMeta::from_headers("1.1.1.1".to_string(), "test/1.0", "", "")
}

async fn whoami(session: Session) -> String {
    session.user_id
}

async fn setup() -> (TestDb, JwtSessionService) {
    let db = TestDb::new().await;
    db.db()
        .conn()
        .execute_raw(TestSession::SCHEMA_SQL, ())
        .await
        .unwrap();
    let mut cfg = JwtSessionsConfig::default();
    cfg.signing_secret = "test-secret-must-be-32-bytes-yes!".into();
    let svc = JwtSessionService::new(db.db(), cfg).unwrap();
    (db, svc)
}

#[tokio::test]
async fn jwt_layer_loads_session_into_extensions() {
    let (_db, svc) = setup().await;
    let pair = svc.authenticate("user_1", &meta()).await.unwrap();

    let app = TestApp::builder()
        .route("/me", get(whoami).route_layer(svc.layer()))
        .build();

    let bearer = format!("Bearer {}", pair.access_token);
    let res = app
        .get("/me")
        .header("Authorization", bearer.as_str())
        .send()
        .await;

    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "user_1");
}

#[tokio::test]
async fn jwt_layer_rejects_after_logout() {
    let (_db, svc) = setup().await;
    let pair = svc.authenticate("user_1", &meta()).await.unwrap();
    svc.logout(&pair.access_token).await.unwrap();

    let app = TestApp::builder()
        .route("/me", get(whoami).route_layer(svc.layer()))
        .build();

    let bearer = format!("Bearer {}", pair.access_token);
    let res = app
        .get("/me")
        .header("Authorization", bearer.as_str())
        .send()
        .await;

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn jwt_layer_rejects_refresh_token_with_401() {
    let (_db, svc) = setup().await;
    let pair = svc.authenticate("user_1", &meta()).await.unwrap();

    let app = TestApp::builder()
        .route("/me", get(whoami).route_layer(svc.layer()))
        .build();

    // Sending a refresh token to a protected route must return 401.
    let bearer = format!("Bearer {}", pair.refresh_token);
    let res = app
        .get("/me")
        .header("Authorization", bearer.as_str())
        .send()
        .await;

    assert_eq!(res.status(), 401);
}
