#![cfg(feature = "test-helpers")]

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::post;
use modo::Result;
use modo::auth::session::jwt::{JwtSession, JwtSessionService, JwtSessionsConfig, TokenPair};
use modo::client::ClientInfo;
use modo::db::ConnExt;
use modo::extractor::JsonRequest;
use modo::sanitize::Sanitize;
use modo::testing::{TestApp, TestDb, TestSession};
use serde::Deserialize;

#[derive(Deserialize)]
struct LoginReq {
    user_id: String,
}

impl Sanitize for LoginReq {
    fn sanitize(&mut self) {
        self.user_id = self.user_id.trim().to_string();
    }
}

fn meta() -> ClientInfo {
    ClientInfo::from_headers(Some("1.1.1.1".to_string()), "test/1.0", "", "")
}

async fn login(
    State(svc): State<JwtSessionService>,
    JsonRequest(req): JsonRequest<LoginReq>,
) -> Result<Json<TokenPair>> {
    Ok(Json(svc.authenticate(&req.user_id, &meta()).await?))
}

async fn refresh(jwt: JwtSession) -> Result<Json<TokenPair>> {
    Ok(Json(jwt.rotate().await?))
}

async fn logout(jwt: JwtSession) -> Result<axum::http::StatusCode> {
    jwt.logout().await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
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
async fn jwt_session_extractor_does_full_lifecycle() {
    let (_db, svc) = setup().await;

    let router: Router = Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .with_state(svc);

    let app = TestApp::from_router(router);

    // 1. Login — get a token pair.
    let res = app
        .post("/login")
        .json(&serde_json::json!({"user_id": "u1"}))
        .send()
        .await;
    assert_eq!(res.status(), 200);
    let pair: TokenPair = res.json();
    assert!(!pair.access_token.is_empty());
    assert!(!pair.refresh_token.is_empty());

    // 2. Refresh — pass the refresh token in the body; get a new pair back.
    let res2 = app
        .post("/refresh")
        .json(&serde_json::json!({"refresh_token": pair.refresh_token}))
        .send()
        .await;
    assert_eq!(res2.status(), 200);
    let pair2: TokenPair = res2.json();
    assert_ne!(pair.refresh_token, pair2.refresh_token);

    // 3. Logout — pass the new access token in the Authorization header.
    let res3 = app
        .post("/logout")
        .header(
            "Authorization",
            format!("Bearer {}", pair2.access_token).as_str(),
        )
        .send()
        .await;
    assert_eq!(res3.status(), 204);
}

#[tokio::test]
async fn jwt_session_extractor_missing_refresh_returns_400() {
    let (_db, svc) = setup().await;

    let router: Router = Router::new()
        .route("/refresh", post(refresh))
        .with_state(svc);

    let app = TestApp::from_router(router);

    // POST with an empty body — no refresh_token field.
    let res = app
        .post("/refresh")
        .json(&serde_json::json!({}))
        .send()
        .await;
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn jwt_session_extractor_missing_access_token_returns_401() {
    let (_db, svc) = setup().await;

    let router: Router = Router::new().route("/logout", post(logout)).with_state(svc);

    let app = TestApp::from_router(router);

    // POST without Authorization header.
    let res = app.post("/logout").send().await;
    assert_eq!(res.status(), 401);
}
