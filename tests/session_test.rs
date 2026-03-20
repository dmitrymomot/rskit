use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use http::StatusCode;
use modo::cookie::{CookieConfig, key_from_config};
use modo::service::Registry;
use modo::session::{Session, SessionConfig, Store};
use tower::ServiceExt;

fn test_cookie_config() -> CookieConfig {
    CookieConfig {
        secret: "a".repeat(64),
        secure: false,
        http_only: true,
        same_site: "lax".to_string(),
    }
}

async fn setup_store() -> (Store, modo::db::Pool) {
    let db_config = modo::db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = modo::db::connect(&db_config).await.unwrap();

    sqlx::query(
        "CREATE TABLE modo_sessions (
            id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE,
            user_id TEXT NOT NULL, ip_address TEXT NOT NULL,
            user_agent TEXT NOT NULL, device_name TEXT NOT NULL,
            device_type TEXT NOT NULL, fingerprint TEXT NOT NULL,
            data TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL,
            last_active_at TEXT NOT NULL, expires_at TEXT NOT NULL
        )",
    )
    .execute(&*pool)
    .await
    .unwrap();

    let store = Store::new(&pool, SessionConfig::default());
    (store, pool)
}

async fn handler_no_auth(session: Session) -> &'static str {
    assert!(!session.is_authenticated());
    "ok"
}

async fn handler_authenticate(session: Session) -> modo::Result<&'static str> {
    session.authenticate("user-123").await?;
    assert!(session.is_authenticated());
    assert_eq!(session.user_id(), Some("user-123".to_string()));
    Ok("ok")
}

async fn handler_logout(session: Session) -> modo::Result<&'static str> {
    session.authenticate("user-123").await?;
    session.logout().await?;
    assert!(!session.is_authenticated());
    Ok("ok")
}

async fn handler_set_get(session: Session) -> modo::Result<&'static str> {
    session.authenticate("user-123").await?;
    session.set("theme", &"dark".to_string())?;
    let theme: Option<String> = session.get("theme")?;
    assert_eq!(theme, Some("dark".to_string()));
    Ok("ok")
}

#[tokio::test]
async fn test_session_middleware_no_cookie_passes_through() {
    let (store, _pool) = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    let app = Router::new()
        .route("/", get(handler_no_auth))
        .layer(modo::session::layer(store, &cookie_config, &key))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_session_authenticate_sets_cookie() {
    let (store, _pool) = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    let app = Router::new()
        .route("/login", post(handler_authenticate))
        .layer(modo::session::layer(store, &cookie_config, &key))
        .with_state(Registry::new().into_state());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response.headers().get("set-cookie");
    assert!(set_cookie.is_some(), "should set session cookie");
}

#[tokio::test]
async fn test_session_logout_removes_cookie() {
    let (store, _pool) = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    let app = Router::new()
        .route("/", post(handler_logout))
        .layer(modo::session::layer(store, &cookie_config, &key))
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

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_session_set_and_get_data() {
    let (store, _pool) = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    let app = Router::new()
        .route("/", post(handler_set_get))
        .layer(modo::session::layer(store, &cookie_config, &key))
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

    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn test_session_config_in_modo_config() {
    let config = modo::Config::default();
    assert_eq!(config.session.session_ttl_secs, 2_592_000);
    assert_eq!(config.session.cookie_name, "_session");
}
