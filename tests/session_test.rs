#![cfg(feature = "session")]

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use http::StatusCode;
use modo::cookie::{CookieConfig, key_from_config};
use modo::db::{self, ConnExt};
use modo::service::Registry;
use modo::session::{Session, SessionConfig, Store};
use tower::ServiceExt;

fn test_cookie_config() -> CookieConfig {
    let mut c = CookieConfig::new("a".repeat(64));
    c.secure = false;
    c
}

async fn setup_store() -> Store {
    let config = db::Config {
        path: ":memory:".into(),
        ..Default::default()
    };
    let db = db::connect(&config).await.unwrap();
    db.conn()
        .execute_raw(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE,
                user_id TEXT NOT NULL, ip_address TEXT NOT NULL,
                user_agent TEXT NOT NULL, device_name TEXT NOT NULL,
                device_type TEXT NOT NULL, fingerprint TEXT NOT NULL,
                data TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL,
                last_active_at TEXT NOT NULL, expires_at TEXT NOT NULL
            )",
            (),
        )
        .await
        .unwrap();
    Store::new(db, SessionConfig::default())
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
    let store = setup_store().await;
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
    let store = setup_store().await;
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
    let store = setup_store().await;
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
    let store = setup_store().await;
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

// ---------------------------------------------------------------------------
// Security-critical integration tests
// ---------------------------------------------------------------------------

use axum::Extension;
use modo::service::AppState;
use modo::session::meta::SessionMeta;

/// Build a Router with the session layer, optionally adding an Extension layer.
/// This helper exists because `oneshot` consumes the service, so each request
/// needs a fresh Router built from the same shared state.
fn build_app(
    store: &Store,
    cookie_config: &CookieConfig,
    key: &modo::cookie::Key,
    route: &str,
    method: axum::routing::MethodRouter<AppState>,
) -> Router {
    Router::new()
        .route(route, method)
        .layer(modo::session::layer(store.clone(), cookie_config, key))
        .with_state(Registry::new().into_state())
}

fn build_app_with_ext<T: Clone + Send + Sync + 'static>(
    store: &Store,
    cookie_config: &CookieConfig,
    key: &modo::cookie::Key,
    route: &str,
    method: axum::routing::MethodRouter<AppState>,
    ext: T,
) -> Router {
    Router::new()
        .route(route, method)
        .layer(Extension(ext))
        .layer(modo::session::layer(store.clone(), cookie_config, key))
        .with_state(Registry::new().into_state())
}

/// Helper: build a default SessionMeta matching the default test user-agent.
fn default_meta() -> SessionMeta {
    SessionMeta::from_headers("127.0.0.1".to_string(), "", "", "")
}

/// Extract the raw Set-Cookie header value from a response.
fn extract_set_cookie(response: &http::Response<Body>) -> Option<String> {
    response
        .headers()
        .get("set-cookie")
        .map(|v| v.to_str().unwrap().to_string())
}

/// Extract just the cookie name=value portion (before the first ';') suitable
/// for sending back in a Cookie header.
fn cookie_header_value(set_cookie: &str) -> String {
    set_cookie
        .split(';')
        .next()
        .unwrap_or(set_cookie)
        .to_string()
}

// --- Module-level handlers for the new tests ---

async fn handler_login(session: Session) -> modo::Result<&'static str> {
    session.authenticate("user-1").await?;
    Ok("ok")
}

async fn handler_check_auth(session: Session) -> String {
    match session.user_id() {
        Some(uid) => uid,
        None => "none".to_string(),
    }
}

async fn handler_logout_all(session: Session) -> modo::Result<&'static str> {
    session.authenticate("user-1").await?;
    session.logout_all().await?;
    Ok("ok")
}

async fn handler_logout_other(session: Session) -> modo::Result<&'static str> {
    session.authenticate("user-1").await?;
    session.logout_other().await?;
    Ok("ok")
}

async fn handler_rotate(session: Session) -> modo::Result<&'static str> {
    session.rotate().await?;
    Ok("ok")
}

async fn handler_list_my_sessions(session: Session) -> modo::Result<&'static str> {
    session.list_my_sessions().await?;
    Ok("ok")
}

async fn handler_authenticate_and_logout(session: Session) -> modo::Result<&'static str> {
    session.authenticate("user-1").await?;
    session.logout().await?;
    Ok("ok")
}

async fn handler_set_unauthenticated(session: Session) -> modo::Result<&'static str> {
    session.set("key", &"val".to_string())?;
    Ok("ok")
}

async fn handler_revoke_ext(
    session: Session,
    Extension(target_id): Extension<String>,
) -> modo::Result<&'static str> {
    session.authenticate("user-a").await?;
    session.revoke(&target_id).await?;
    Ok("ok")
}

// ---------------------------------------------------------------------------
// Test 1: Cookie round-trip — authenticate, extract Set-Cookie, send back
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_cookie_round_trip() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    // Step 1: POST /login to authenticate and obtain the session cookie.
    let app1 = build_app(&store, &cookie_config, &key, "/login", post(handler_login));
    let resp = app1
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let set_cookie = extract_set_cookie(&resp).expect("login must set session cookie");
    let cookie_val = cookie_header_value(&set_cookie);

    // Step 2: GET /check with the session cookie — should see user-1.
    let app2 = build_app(
        &store,
        &cookie_config,
        &key,
        "/check",
        get(handler_check_auth),
    );
    let resp = app2
        .oneshot(
            Request::builder()
                .uri("/check")
                .header("cookie", &cookie_val)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "user-1",
        "second request must see the authenticated user"
    );
}

// ---------------------------------------------------------------------------
// Test 2: Fingerprint mismatch destroys session
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_fingerprint_mismatch_destroys_session() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    // Login with User-Agent "Chrome"
    let app1 = build_app(&store, &cookie_config, &key, "/login", post(handler_login));
    let resp = app1
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header("user-agent", "Chrome/100")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let set_cookie = extract_set_cookie(&resp).expect("must set cookie");
    let cookie_val = cookie_header_value(&set_cookie);

    // Second request with DIFFERENT User-Agent — fingerprint won't match.
    let app2 = build_app(
        &store,
        &cookie_config,
        &key,
        "/check",
        get(handler_check_auth),
    );
    let resp = app2
        .oneshot(
            Request::builder()
                .uri("/check")
                .header("cookie", &cookie_val)
                .header("user-agent", "Firefox/120")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "none",
        "fingerprint mismatch must destroy the session"
    );

    // Verify the session was actually deleted from the store.
    let sessions = store.list_for_user("user-1").await.unwrap();
    assert!(
        sessions.is_empty(),
        "store must have no sessions after fingerprint mismatch"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Re-authentication destroys the old session (fixation prevention)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_authenticate_destroys_old_session() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    // First login
    let app1 = build_app(&store, &cookie_config, &key, "/login", post(handler_login));
    let resp1 = app1
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let set_cookie1 = extract_set_cookie(&resp1).expect("first login must set cookie");

    // Second login (fresh request, no cookie — simulates new authentication)
    let app2 = build_app(&store, &cookie_config, &key, "/login", post(handler_login));
    let resp2 = app2
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let set_cookie2 = extract_set_cookie(&resp2).expect("second login must set cookie");

    // Tokens must differ — proves a new session was created.
    assert_ne!(
        cookie_header_value(&set_cookie1),
        cookie_header_value(&set_cookie2),
        "re-authentication must produce a new session token"
    );

    // There should be exactly 2 sessions (each login creates one; old one from
    // first call isn't destroyed because second call has no cookie to identify it).
    // But if we login WITH the first cookie, the old session IS destroyed.
    let app3 = build_app(&store, &cookie_config, &key, "/login", post(handler_login));
    let resp3 = app3
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header("cookie", &cookie_header_value(&set_cookie2))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let set_cookie3 = extract_set_cookie(&resp3).expect("third login must set cookie");

    assert_ne!(
        cookie_header_value(&set_cookie2),
        cookie_header_value(&set_cookie3),
        "re-auth with existing session must issue a new token"
    );

    // Verify the old session (from set_cookie2) no longer loads.
    let app4 = build_app(
        &store,
        &cookie_config,
        &key,
        "/check",
        get(handler_check_auth),
    );
    let resp4 = app4
        .oneshot(
            Request::builder()
                .uri("/check")
                .header("cookie", &cookie_header_value(&set_cookie2))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(resp4.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "none",
        "old session must be destroyed after re-authentication"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Revoking another user's session returns Not Found
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_revoke_other_users_session_returns_not_found() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    // Pre-create a session for user-b directly via the store.
    let meta_b = default_meta();
    let (session_b, _token_b) = store.create(&meta_b, "user-b", None).await.unwrap();

    // user-a authenticates and tries to revoke user-b's session.
    let app = build_app_with_ext(
        &store,
        &cookie_config,
        &key,
        "/revoke",
        post(handler_revoke_ext),
        session_b.id.clone(),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/revoke")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "revoking another user's session must return 404"
    );

    // Verify user-b's session is still intact.
    let still_exists = store.read(&session_b.id).await.unwrap();
    assert!(
        still_exists.is_some(),
        "user-b's session must not be deleted by user-a's revoke attempt"
    );
}

// ---------------------------------------------------------------------------
// Test 5: logout_all destroys all sessions for the user
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_logout_all_destroys_all_sessions() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    // Pre-create 2 sessions for user-1 directly.
    let meta = default_meta();
    store.create(&meta, "user-1", None).await.unwrap();
    store.create(&meta, "user-1", None).await.unwrap();

    // Authenticate (creates a 3rd session), then logout_all.
    let app = build_app(
        &store,
        &cookie_config,
        &key,
        "/logout-all",
        post(handler_logout_all),
    );
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/logout-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let remaining = store.list_for_user("user-1").await.unwrap();
    assert!(
        remaining.is_empty(),
        "logout_all must destroy every session for the user, found {}",
        remaining.len()
    );
}

// ---------------------------------------------------------------------------
// Test 6: logout_other keeps the current session, destroys the rest
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_logout_other_keeps_current() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    // Pre-create 1 extra session for user-1.
    let meta = default_meta();
    store.create(&meta, "user-1", None).await.unwrap();

    // Authenticate (creates current session), then logout_other.
    let app = build_app(
        &store,
        &cookie_config,
        &key,
        "/logout-other",
        post(handler_logout_other),
    );
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/logout-other")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let remaining = store.list_for_user("user-1").await.unwrap();
    assert_eq!(
        remaining.len(),
        1,
        "logout_other must keep exactly the current session"
    );
}

// ---------------------------------------------------------------------------
// Test 7: rotate() on unauthenticated session returns 401
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_rotate_unauthenticated_returns_error() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    let app = build_app(
        &store,
        &cookie_config,
        &key,
        "/rotate",
        post(handler_rotate),
    );
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rotate")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "rotate on unauthenticated session must return 401"
    );
}

// ---------------------------------------------------------------------------
// Test 8: list_my_sessions() on unauthenticated session returns 401
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_list_my_sessions_unauthenticated_returns_error() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    let app = build_app(
        &store,
        &cookie_config,
        &key,
        "/list",
        get(handler_list_my_sessions),
    );
    let resp = app
        .oneshot(Request::builder().uri("/list").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "list_my_sessions on unauthenticated session must return 401"
    );
}

// ---------------------------------------------------------------------------
// Test 9: logout sets Max-Age=0 in the response cookie (removes cookie)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_logout_removes_cookie_in_response() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    let app = build_app(
        &store,
        &cookie_config,
        &key,
        "/auth-then-logout",
        post(handler_authenticate_and_logout),
    );
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth-then-logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    // The middleware processes the Remove action, which calls remove_signed_cookie
    // with max_age=0. Collect all Set-Cookie headers to find the removal cookie.
    let set_cookies: Vec<String> = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();

    // There should be at least one Set-Cookie with Max-Age=0.
    let has_removal = set_cookies
        .iter()
        .any(|c| c.contains("Max-Age=0") || c.contains("max-age=0"));
    assert!(
        has_removal,
        "logout must produce a Set-Cookie with Max-Age=0 to clear the cookie; got: {set_cookies:?}"
    );
}

// ---------------------------------------------------------------------------
// Helpers for the new tests
// ---------------------------------------------------------------------------

async fn setup_store_with_config(config: SessionConfig) -> Store {
    let db_config = db::Config {
        path: ":memory:".into(),
        ..Default::default()
    };
    let db = db::connect(&db_config).await.unwrap();
    db.conn()
        .execute_raw(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY, token_hash TEXT NOT NULL UNIQUE,
                user_id TEXT NOT NULL, ip_address TEXT NOT NULL,
                user_agent TEXT NOT NULL, device_name TEXT NOT NULL,
                device_type TEXT NOT NULL, fingerprint TEXT NOT NULL,
                data TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL,
                last_active_at TEXT NOT NULL, expires_at TEXT NOT NULL
            )",
            (),
        )
        .await
        .unwrap();
    Store::new(db, config)
}

// Module-level handlers for new tests

/// Set "color" = "blue" on an already-authenticated session.
async fn handler_set_color(session: Session) -> modo::Result<&'static str> {
    session.set("color", &"blue".to_string())?;
    Ok("ok")
}

async fn handler_get_color(session: Session) -> modo::Result<String> {
    let color: Option<String> = session.get("color")?;
    Ok(color.unwrap_or_else(|| "none".to_string()))
}

async fn handler_rotate_only(session: Session) -> modo::Result<&'static str> {
    session.rotate().await?;
    Ok("ok")
}

async fn handler_authenticate_with_role(session: Session) -> modo::Result<&'static str> {
    session
        .authenticate_with("user-role", serde_json::json!({"role": "admin"}))
        .await?;
    Ok("ok")
}

async fn handler_get_role(session: Session) -> modo::Result<String> {
    let role: Option<String> = session.get("role")?;
    Ok(role.unwrap_or_else(|| "none".to_string()))
}

async fn handler_set_key_then_remove(session: Session) -> modo::Result<&'static str> {
    session.authenticate("user-remove").await?;
    session.set("key", &"value".to_string())?;
    Ok("ok")
}

async fn handler_remove_key_and_read(session: Session) -> modo::Result<String> {
    session.remove_key("key");
    let val: Option<String> = session.get("key")?;
    Ok(val.unwrap_or_else(|| "none".to_string()))
}

async fn handler_current(session: Session) -> modo::Result<String> {
    match session.current() {
        Some(data) => Ok(data.user_id),
        None => Ok("none".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Test 11: Session data persists across requests
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_session_data_persists_across_requests() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    // Request 1: authenticate to get a session cookie
    let app1 = build_app(&store, &cookie_config, &key, "/login", post(handler_login));
    let resp1 = app1
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let cookie_val =
        cookie_header_value(&extract_set_cookie(&resp1).expect("must set session cookie"));

    // Request 2: set "color" = "blue" on the existing authenticated session
    // (handler_set_color does NOT call authenticate — just calls session.set)
    let app2 = build_app(
        &store,
        &cookie_config,
        &key,
        "/set-color",
        post(handler_set_color),
    );
    let resp2 = app2
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/set-color")
                .header("cookie", &cookie_val)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    // Cookie may or may not be refreshed; use the original cookie_val — it is still valid
    // (rotate was not called, same session ID)

    // Request 3: read "color" from session
    let app3 = build_app(
        &store,
        &cookie_config,
        &key,
        "/get-color",
        get(handler_get_color),
    );
    let resp3 = app3
        .oneshot(
            Request::builder()
                .uri("/get-color")
                .header("cookie", &cookie_val)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp3.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp3.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "blue",
        "session data must persist across requests"
    );
}

// ---------------------------------------------------------------------------
// Test 12: Rotate invalidates old token; new token is valid
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_rotate_authenticated_session() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    // Step 1: authenticate, obtain cookie A
    let app1 = build_app(&store, &cookie_config, &key, "/login", post(handler_login));
    let resp1 = app1
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let cookie_a = cookie_header_value(&extract_set_cookie(&resp1).expect("cookie A required"));

    // Step 2: rotate using cookie A, obtain cookie B
    let app2 = build_app(
        &store,
        &cookie_config,
        &key,
        "/rotate",
        post(handler_rotate_only),
    );
    let resp2 = app2
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rotate")
                .header("cookie", &cookie_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let cookie_b = cookie_header_value(&extract_set_cookie(&resp2).expect("cookie B required"));

    // cookie A and cookie B must differ
    assert_ne!(cookie_a, cookie_b, "rotate must issue a new cookie");

    // Step 3: old cookie A should no longer load a session
    let app3 = build_app(
        &store,
        &cookie_config,
        &key,
        "/check",
        get(handler_check_auth),
    );
    let resp3 = app3
        .oneshot(
            Request::builder()
                .uri("/check")
                .header("cookie", &cookie_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(resp3.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "none",
        "old cookie must be invalid after rotate"
    );

    // Step 4: new cookie B should load the session successfully
    let app4 = build_app(
        &store,
        &cookie_config,
        &key,
        "/check",
        get(handler_check_auth),
    );
    let resp4 = app4
        .oneshot(
            Request::builder()
                .uri("/check")
                .header("cookie", &cookie_b)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(resp4.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "user-1",
        "new cookie must be valid after rotate"
    );
}

// ---------------------------------------------------------------------------
// Test 13: authenticate_with stores initial data readable in next request
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_session_authenticate_with_initial_data() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    // Request 1: authenticate_with initial role=admin
    let app1 = build_app(
        &store,
        &cookie_config,
        &key,
        "/auth-with",
        post(handler_authenticate_with_role),
    );
    let resp1 = app1
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth-with")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let cookie_val = cookie_header_value(&extract_set_cookie(&resp1).expect("cookie required"));

    // Request 2: read "role" from session
    let app2 = build_app(
        &store,
        &cookie_config,
        &key,
        "/get-role",
        get(handler_get_role),
    );
    let resp2 = app2
        .oneshot(
            Request::builder()
                .uri("/get-role")
                .header("cookie", &cookie_val)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp2.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "admin",
        "authenticate_with initial data must persist to next request"
    );
}

// ---------------------------------------------------------------------------
// Test 14: remove_key removes a previously set key
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_session_remove_key() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    // Request 1: authenticate and set "key" = "value"
    let app1 = build_app(
        &store,
        &cookie_config,
        &key,
        "/set",
        post(handler_set_key_then_remove),
    );
    let resp1 = app1
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/set")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let cookie_val = cookie_header_value(&extract_set_cookie(&resp1).expect("cookie required"));

    // Request 2: remove_key("key") and then get("key") should be None
    let app2 = build_app(
        &store,
        &cookie_config,
        &key,
        "/remove",
        post(handler_remove_key_and_read),
    );
    let resp2 = app2
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/remove")
                .header("cookie", &cookie_val)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp2.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "none",
        "remove_key must remove the value so get returns None"
    );
}

// ---------------------------------------------------------------------------
// Test 15: session.current() returns the authenticated user's data
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_session_current() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    // Request 1: authenticate as user-current
    let app1 = build_app(&store, &cookie_config, &key, "/login", post(handler_login));
    let resp1 = app1
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let cookie_val = cookie_header_value(&extract_set_cookie(&resp1).expect("cookie required"));

    // Request 2: call current() and verify user_id
    let app2 = build_app(
        &store,
        &cookie_config,
        &key,
        "/current",
        get(handler_current),
    );
    let resp2 = app2
        .oneshot(
            Request::builder()
                .uri("/current")
                .header("cookie", &cookie_val)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp2.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "user-1",
        "current() must return the authenticated user's data"
    );
}

// ---------------------------------------------------------------------------
// Test 16: validate_fingerprint=false allows different User-Agent in subsequent requests
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_validate_fingerprint_disabled() {
    let mut config = SessionConfig::default();
    config.validate_fingerprint = false;
    let store = setup_store_with_config(config).await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    // Request 1: authenticate with User-Agent "Chrome/100"
    let app1 = build_app(&store, &cookie_config, &key, "/login", post(handler_login));
    let resp1 = app1
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header("user-agent", "Chrome/100")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let cookie_val = cookie_header_value(&extract_set_cookie(&resp1).expect("cookie required"));

    // Request 2: send a DIFFERENT User-Agent — fingerprint check is disabled, so session is valid
    let app2 = build_app(
        &store,
        &cookie_config,
        &key,
        "/check",
        get(handler_check_auth),
    );
    let resp2 = app2
        .oneshot(
            Request::builder()
                .uri("/check")
                .header("cookie", &cookie_val)
                .header("user-agent", "Firefox/120")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp2.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        std::str::from_utf8(&body).unwrap(),
        "user-1",
        "with validate_fingerprint=false, different User-Agent must not destroy the session"
    );
}

// ---------------------------------------------------------------------------
// Test 10: set() is a no-op when unauthenticated (no session in store, no cookie set)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_set_no_op_when_unauthenticated() {
    let store = setup_store().await;
    let cookie_config = test_cookie_config();
    let key = key_from_config(&cookie_config).unwrap();

    let app = build_app(
        &store,
        &cookie_config,
        &key,
        "/set",
        post(handler_set_unauthenticated),
    );
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/set")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "set() on unauthenticated session must succeed silently"
    );

    let set_cookie = resp.headers().get("set-cookie");
    assert!(
        set_cookie.is_none(),
        "set() on unauthenticated session must not produce a Set-Cookie header"
    );

    // Verify nothing was persisted in the store.
    let all_sessions = store.list_for_user("").await.unwrap();
    assert!(
        all_sessions.is_empty(),
        "no session should be created when set() is called without authentication"
    );
}
