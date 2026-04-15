#![cfg(feature = "test-helpers")]

use modo::auth::session::cookie::{CookieSessionService, CookieSessionsConfig};
use modo::auth::session::jwt::{JwtSessionService, JwtSessionsConfig};
use modo::auth::session::meta::SessionMeta;
use modo::db::ConnExt;
use modo::testing::{TestDb, TestSession};

fn meta() -> SessionMeta {
    SessionMeta::from_headers("1.1.1.1".to_string(), "test/1.0", "", "")
}

async fn setup() -> (TestDb, CookieSessionService, JwtSessionService) {
    let db = TestDb::new().await;
    db.db()
        .conn()
        .execute_raw(TestSession::SCHEMA_SQL, ())
        .await
        .unwrap();
    for sql in TestSession::INDEXES_SQL {
        db.db().conn().execute_raw(sql, ()).await.unwrap();
    }

    let mut cookie_cfg = CookieSessionsConfig::default();
    cookie_cfg.cookie.secret = "a".repeat(64);
    let cookies = CookieSessionService::new(db.db(), cookie_cfg).unwrap();

    let mut jwt_cfg = JwtSessionsConfig::default();
    jwt_cfg.signing_secret = "test-secret-32-bytes-long-okay-??".into();
    let jwts = JwtSessionService::new(db.db(), jwt_cfg).unwrap();

    (db, cookies, jwts)
}

#[tokio::test]
async fn revoke_all_from_jwt_wipes_cookie_rows_too() {
    let (_db, cookies, jwts) = setup().await;

    // Create a cookie-side row via the store.
    let meta = meta();
    cookies.store().create(&meta, "user_1", None).await.unwrap();

    // Create a JWT-side row via authenticate.
    jwts.authenticate("user_1", &meta).await.unwrap();

    // Both services share the same table — each should see 2 rows.
    let from_cookie = cookies.list("user_1").await.unwrap();
    let from_jwt = jwts.list("user_1").await.unwrap();
    assert_eq!(
        from_cookie.len(),
        2,
        "cookie service should see 2 rows before revoke"
    );
    assert_eq!(
        from_jwt.len(),
        2,
        "jwt service should see 2 rows before revoke"
    );

    // Revoking all via the JWT service must also wipe cookie-backed rows.
    jwts.revoke_all("user_1").await.unwrap();

    let after_cookie = cookies.list("user_1").await.unwrap();
    let after_jwt = jwts.list("user_1").await.unwrap();
    assert_eq!(
        after_cookie.len(),
        0,
        "cookie rows should be gone after jwt revoke_all"
    );
    assert_eq!(
        after_jwt.len(),
        0,
        "jwt rows should be gone after jwt revoke_all"
    );
}

#[tokio::test]
async fn revoke_all_from_cookie_wipes_jwt_rows_too() {
    let (_db, cookies, jwts) = setup().await;

    let meta = meta();
    // Create a JWT-side row via authenticate.
    jwts.authenticate("user_1", &meta).await.unwrap();

    // Create a cookie-side row via the store.
    cookies.store().create(&meta, "user_1", None).await.unwrap();

    // Both services share the same table — each should see 2 rows.
    let from_cookie = cookies.list("user_1").await.unwrap();
    let from_jwt = jwts.list("user_1").await.unwrap();
    assert_eq!(
        from_cookie.len(),
        2,
        "cookie service should see 2 rows before revoke"
    );
    assert_eq!(
        from_jwt.len(),
        2,
        "jwt service should see 2 rows before revoke"
    );

    // Revoking all via the cookie service must also wipe JWT-backed rows.
    cookies.revoke_all("user_1").await.unwrap();

    let after_cookie = cookies.list("user_1").await.unwrap();
    let after_jwt = jwts.list("user_1").await.unwrap();
    assert_eq!(
        after_cookie.len(),
        0,
        "cookie rows should be gone after cookie revoke_all"
    );
    assert_eq!(
        after_jwt.len(),
        0,
        "jwt rows should be gone after cookie revoke_all"
    );
}
