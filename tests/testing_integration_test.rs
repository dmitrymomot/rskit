#![cfg(feature = "test-helpers")]

use axum::Json;
use axum::routing::get;
use modo::auth::session::Session;
use modo::db::{ColumnMap, ConnExt, ConnQueryExt, Database, FromRow};
use modo::testing::{TestApp, TestDb, TestSession};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct User {
    id: String,
    name: String,
}

impl FromRow for User {
    fn from_row(row: &libsql::Row) -> modo::Result<Self> {
        let cols = ColumnMap::from_row(row);
        Ok(Self {
            id: cols.get(row, "id")?,
            name: cols.get(row, "name")?,
        })
    }
}

impl modo::sanitize::Sanitize for User {
    fn sanitize(&mut self) {}
}

async fn create_user(
    _session: Session,
    modo::service::Service(db): modo::service::Service<Database>,
    modo::extractor::JsonRequest(input): modo::extractor::JsonRequest<User>,
) -> modo::Result<Json<User>> {
    db.conn()
        .execute_raw(
            "INSERT INTO users (id, name) VALUES (?1, ?2)",
            libsql::params![input.id.clone(), input.name.clone()],
        )
        .await
        .map_err(|e| modo::Error::internal(e.to_string()))?;
    Ok(Json(input))
}

async fn list_users(
    _session: Session,
    modo::service::Service(db): modo::service::Service<Database>,
) -> modo::Result<Json<Vec<User>>> {
    let users: Vec<User> = db
        .conn()
        .query_all("SELECT id, name FROM users", ())
        .await?;
    Ok(Json(users))
}

async fn whoami(session: Option<Session>) -> String {
    session
        .map(|s| s.user_id)
        .unwrap_or_else(|| "anonymous".to_string())
}

#[tokio::test]
async fn test_full_app_with_db_and_session() {
    let db = TestDb::new()
        .await
        .exec("CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .await;
    let session = TestSession::new(&db).await;

    let app = TestApp::builder()
        .service(db.db())
        .route("/users", get(list_users).post(create_user))
        .route("/me", get(whoami))
        .layer(session.layer())
        .build();

    // Unauthenticated
    let res = app.get("/me").send().await;
    assert_eq!(res.text(), "anonymous");

    // Authenticate
    let cookie = session.authenticate("user-1").await;

    // Create user
    let res = app
        .post("/users")
        .header("cookie", &cookie)
        .json(&User {
            id: "1".to_string(),
            name: "Alice".to_string(),
        })
        .send()
        .await;
    assert_eq!(res.status(), 200);

    // List users
    let res = app.get("/users").header("cookie", &cookie).send().await;
    assert_eq!(res.status(), 200);
    let users: Vec<User> = res.json();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name, "Alice");

    // Check identity
    let res = app.get("/me").header("cookie", &cookie).send().await;
    assert_eq!(res.text(), "user-1");
}
