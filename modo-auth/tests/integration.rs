// Force the linker to include modo_session entity registration.
#[allow(unused_imports)]
use modo_session::entity::session as _;

use axum::Router;
use axum::routing::get;
use http::Request;
use modo::{AppState, ServerConfig, ServiceRegistry};
use modo_auth::{Auth, OptionalAuth, UserProvider, UserProviderService};
use modo_db::sea_orm::{ConnectionTrait, Schema};
use modo_db::{DatabaseConfig, DbPool};
use modo_session::{SessionConfig, SessionMeta, SessionStore};
use tower::ServiceExt;

// --- Test user & providers ---

#[derive(Clone, Debug)]
struct TestUser {
    name: String,
}

struct TestProvider;

impl UserProvider for TestProvider {
    type User = TestUser;

    async fn find_by_id(&self, id: &str) -> Result<Option<Self::User>, modo::Error> {
        match id {
            "user-1" => Ok(Some(TestUser {
                name: "Alice".into(),
            })),
            "error-user" => Err(modo::Error::internal("db error")),
            _ => Ok(None),
        }
    }
}

// --- Helpers ---

async fn setup_db() -> DbPool {
    let config = DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        max_connections: 1,
        min_connections: 1,
        ..Default::default()
    };
    let db = modo_db::connect(&config).await.expect("Failed to connect");

    let schema = Schema::new(db.connection().get_database_backend());
    let mut builder = schema.builder();
    let reg = modo_db::inventory::iter::<modo_db::EntityRegistration>()
        .find(|r| r.table_name == "modo_sessions")
        .expect("modo_sessions entity not registered");
    builder = (reg.register_fn)(builder);
    builder
        .sync(db.connection())
        .await
        .expect("Schema sync failed");
    for sql in reg.extra_sql {
        db.connection()
            .execute_unprepared(sql)
            .await
            .expect("Extra SQL failed");
    }
    db
}

fn test_meta() -> SessionMeta {
    SessionMeta::from_headers(
        "127.0.0.1".to_string(),
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0.0.0",
        "en-US",
        "gzip",
    )
}

fn build_app(store: SessionStore) -> Router {
    let services = ServiceRegistry::new()
        .with(UserProviderService::new(TestProvider))
        .with(store.clone());

    let state = AppState {
        services,
        server_config: ServerConfig::default(),
        cookie_key: axum_extra::extract::cookie::Key::generate(),
    };

    Router::new()
        .route("/auth", get(auth_handler))
        .route("/optional", get(optional_handler))
        .layer(modo_session::layer(store))
        .with_state(state)
}

async fn auth_handler(Auth(user): Auth<TestUser>) -> String {
    user.name
}

async fn optional_handler(OptionalAuth(user): OptionalAuth<TestUser>) -> String {
    user.map(|u| u.name).unwrap_or_else(|| "guest".into())
}

/// Create a session and return the cookie header value.
async fn create_session(store: &SessionStore, user_id: &str) -> String {
    let meta = test_meta();
    let (_session, token) = store.create(&meta, user_id, None).await.unwrap();
    format!(
        "{}={}",
        SessionConfig::default().cookie_name,
        token.as_hex()
    )
}

fn request_with_cookie(uri: &str, cookie: &str) -> Request<axum::body::Body> {
    Request::builder()
        .uri(uri)
        .header("cookie", cookie)
        .header(
            "user-agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0.0.0",
        )
        .header("accept-language", "en-US")
        .header("accept-encoding", "gzip")
        .body(axum::body::Body::empty())
        .unwrap()
}

fn request_no_cookie(uri: &str) -> Request<axum::body::Body> {
    Request::builder()
        .uri(uri)
        .header(
            "user-agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0.0.0",
        )
        .header("accept-language", "en-US")
        .header("accept-encoding", "gzip")
        .body(axum::body::Body::empty())
        .unwrap()
}

// --- Auth<U> tests ---

#[tokio::test]
async fn auth_no_session_returns_401() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());
    let app = build_app(store);

    let resp = app.oneshot(request_no_cookie("/auth")).await.unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn auth_valid_session_returns_user() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());
    let app = build_app(store.clone());

    let cookie = create_session(&store, "user-1").await;
    let resp = app
        .oneshot(request_with_cookie("/auth", &cookie))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"Alice");
}

#[tokio::test]
async fn auth_user_not_found_returns_401() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());
    let app = build_app(store.clone());

    let cookie = create_session(&store, "unknown-user").await;
    let resp = app
        .oneshot(request_with_cookie("/auth", &cookie))
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn auth_provider_error_returns_500() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());
    let app = build_app(store.clone());

    let cookie = create_session(&store, "error-user").await;
    let resp = app
        .oneshot(request_with_cookie("/auth", &cookie))
        .await
        .unwrap();
    assert_eq!(resp.status(), 500);
}

// --- OptionalAuth<U> tests ---

#[tokio::test]
async fn optional_auth_no_session_returns_none() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());
    let app = build_app(store);

    let resp = app.oneshot(request_no_cookie("/optional")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"guest");
}

#[tokio::test]
async fn optional_auth_valid_session_returns_user() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());
    let app = build_app(store.clone());

    let cookie = create_session(&store, "user-1").await;
    let resp = app
        .oneshot(request_with_cookie("/optional", &cookie))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"Alice");
}

#[tokio::test]
async fn optional_auth_provider_error_returns_500() {
    let db = setup_db().await;
    let store = SessionStore::new(&db, SessionConfig::default(), Default::default());
    let app = build_app(store.clone());

    let cookie = create_session(&store, "error-user").await;
    let resp = app
        .oneshot(request_with_cookie("/optional", &cookie))
        .await
        .unwrap();
    assert_eq!(resp.status(), 500);
}
