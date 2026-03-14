use modo_db::db_err_to_error;
use modo_db::sea_orm::{ConnectionTrait, Database};
use sea_orm::DbErr;

#[test]
fn record_not_found_maps_to_404() {
    let err = db_err_to_error(DbErr::RecordNotFound("test".into()));
    assert_eq!(err.status_code(), modo::axum::http::StatusCode::NOT_FOUND);
}

#[test]
fn other_errors_map_to_500() {
    let err = db_err_to_error(DbErr::Custom("boom".into()));
    assert_eq!(
        err.status_code(),
        modo::axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn unique_constraint_maps_to_409() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.execute_unprepared(
        "CREATE TABLE test_unique (id INTEGER PRIMARY KEY, email TEXT NOT NULL UNIQUE)",
    )
    .await
    .unwrap();
    db.execute_unprepared("INSERT INTO test_unique (id, email) VALUES (1, 'test@example.com')")
        .await
        .unwrap();

    // Try to insert a duplicate email — should trigger UNIQUE constraint violation
    let result = db
        .execute_unprepared("INSERT INTO test_unique (id, email) VALUES (2, 'test@example.com')")
        .await;

    assert!(result.is_err(), "duplicate insert should fail");
    let err = db_err_to_error(result.unwrap_err());
    assert_eq!(
        err.status_code(),
        modo::axum::http::StatusCode::CONFLICT,
        "unique constraint violation should map to 409 Conflict"
    );
}

#[tokio::test]
async fn foreign_key_constraint_maps_to_409() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    // Enable foreign key enforcement for SQLite
    db.execute_unprepared("PRAGMA foreign_keys = ON")
        .await
        .unwrap();
    db.execute_unprepared("CREATE TABLE fk_parents (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .await
        .unwrap();
    db.execute_unprepared(
        "CREATE TABLE fk_children (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER NOT NULL,
            FOREIGN KEY (parent_id) REFERENCES fk_parents(id)
        )",
    )
    .await
    .unwrap();
    db.execute_unprepared("INSERT INTO fk_parents (id, name) VALUES (1, 'parent')")
        .await
        .unwrap();
    db.execute_unprepared("INSERT INTO fk_children (id, parent_id) VALUES (1, 1)")
        .await
        .unwrap();

    // Try to delete the parent while a child references it
    let result = db
        .execute_unprepared("DELETE FROM fk_parents WHERE id = 1")
        .await;

    assert!(
        result.is_err(),
        "deleting referenced parent should fail with FK constraint"
    );
    let err = db_err_to_error(result.unwrap_err());
    assert_eq!(
        err.status_code(),
        modo::axum::http::StatusCode::CONFLICT,
        "foreign key constraint violation should map to 409 Conflict"
    );
}
