use modo_db::Record;
use modo_db::sea_orm::{ConnectionTrait, Database, DatabaseConnection};

// Force inventory registration of the test entity
#[allow(unused_imports)]
use test_record as _;

// -- Test entity definition ---------------------------------------------------

#[modo_db::entity(table = "test_records")]
#[entity(timestamps)]
pub struct TestRecord {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub name: String,
    #[entity(default_value = 0)]
    pub score: i32,
}

// -- Setup helper -------------------------------------------------------------

async fn setup_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS test_records (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            score INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .await
    .unwrap();
    db
}

// -- Tests --------------------------------------------------------------------

#[tokio::test]
async fn test_insert_and_find_by_id() {
    let db = setup_db().await;

    let record = TestRecord {
        name: "alice".to_string(),
        score: 42,
        ..Default::default()
    };
    let inserted = record.insert(&db).await.unwrap();

    assert!(!inserted.id.is_empty(), "id should be auto-generated");
    assert_eq!(inserted.name, "alice");
    assert_eq!(inserted.score, 42);

    let found = TestRecord::find_by_id(&inserted.id, &db).await.unwrap();
    assert_eq!(found.id, inserted.id);
    assert_eq!(found.name, "alice");
    assert_eq!(found.score, 42);
}

#[tokio::test]
async fn test_update() {
    let db = setup_db().await;

    let record = TestRecord {
        name: "before".to_string(),
        score: 1,
        ..Default::default()
    };
    let mut record = record.insert(&db).await.unwrap();
    let id = record.id.clone();

    record.name = "after".to_string();
    record.score = 99;
    record.update(&db).await.unwrap();

    // record is mutated in-place with refreshed values
    assert_eq!(record.id, id);
    assert_eq!(record.name, "after");
    assert_eq!(record.score, 99);

    let found = TestRecord::find_by_id(&id, &db).await.unwrap();
    assert_eq!(found.name, "after");
    assert_eq!(found.score, 99);
}

#[tokio::test]
async fn test_delete() {
    let db = setup_db().await;

    let record = TestRecord {
        name: "to_delete".to_string(),
        ..Default::default()
    };
    let inserted = record.insert(&db).await.unwrap();
    let id = inserted.id.clone();

    inserted.delete(&db).await.unwrap();

    let result = TestRecord::find_by_id(&id, &db).await;
    assert!(result.is_err(), "should return error after deletion");
    let err = result.unwrap_err();
    assert_eq!(
        err.status_code(),
        modo::axum::http::StatusCode::NOT_FOUND,
        "error should be 404 Not Found"
    );
}

#[tokio::test]
async fn test_delete_by_id() {
    let db = setup_db().await;

    let record = TestRecord {
        name: "to_delete_by_id".to_string(),
        ..Default::default()
    };
    let inserted = record.insert(&db).await.unwrap();
    let id = inserted.id.clone();

    TestRecord::delete_by_id(&id, &db).await.unwrap();

    let result = TestRecord::find_by_id(&id, &db).await;
    assert!(result.is_err(), "should return error after delete_by_id");
    let err = result.unwrap_err();
    assert_eq!(
        err.status_code(),
        modo::axum::http::StatusCode::NOT_FOUND,
        "error should be 404 Not Found"
    );
}

#[tokio::test]
async fn test_find_all() {
    let db = setup_db().await;

    for i in 0..3 {
        TestRecord {
            name: format!("record_{i}"),
            score: i,
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
    }

    let all = TestRecord::find_all(&db).await.unwrap();
    assert_eq!(all.len(), 3, "should find all 3 records");
}

#[tokio::test]
async fn test_query_with_filter() {
    let db = setup_db().await;

    for i in 0..5_i32 {
        TestRecord {
            name: format!("item_{i}"),
            score: i,
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
    }

    use modo_db::sea_orm::ColumnTrait;
    let filtered = TestRecord::query()
        .filter(test_record::Column::Score.gt(2))
        .all(&db)
        .await
        .unwrap();

    assert_eq!(filtered.len(), 2, "should return records with score > 2");
    assert!(
        filtered.iter().all(|r| r.score > 2),
        "all returned records should have score > 2"
    );
}

#[tokio::test]
async fn test_query_one_some() {
    let db = setup_db().await;

    TestRecord {
        name: "unique_name".to_string(),
        score: 7,
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    use modo_db::sea_orm::ColumnTrait;
    let result = TestRecord::query()
        .filter(test_record::Column::Name.eq("unique_name"))
        .one(&db)
        .await
        .unwrap();

    assert!(result.is_some(), "should return Some for existing record");
    let found = result.unwrap();
    assert_eq!(found.name, "unique_name");
    assert_eq!(found.score, 7);
}

#[tokio::test]
async fn test_query_one_none() {
    let db = setup_db().await;

    use modo_db::sea_orm::ColumnTrait;
    let result = TestRecord::query()
        .filter(test_record::Column::Name.eq("nonexistent"))
        .one(&db)
        .await
        .unwrap();

    assert!(
        result.is_none(),
        "should return None for nonexistent record"
    );
}

#[tokio::test]
async fn test_find_by_id_not_found() {
    let db = setup_db().await;

    let result = TestRecord::find_by_id("nonexistent_id_xyz", &db).await;
    assert!(result.is_err(), "should return error for nonexistent id");
    let err = result.unwrap_err();
    assert_eq!(
        err.status_code(),
        modo::axum::http::StatusCode::NOT_FOUND,
        "error should be 404 Not Found"
    );
}

#[tokio::test]
async fn test_update_many() {
    let db = setup_db().await;

    for i in 0..4_i32 {
        TestRecord {
            name: format!("bulk_{i}"),
            score: i * 10,
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
    }

    use modo_db::sea_orm::{ColumnTrait, sea_query::Expr};
    let affected = TestRecord::update_many()
        .filter(test_record::Column::Score.gt(15))
        .col_expr(test_record::Column::Score, Expr::value(0_i32))
        .exec(&db)
        .await
        .unwrap();

    // records with score 20 and 30 should be updated (indices 2 and 3)
    assert_eq!(affected, 2, "should update 2 records");

    let zeroed = TestRecord::query()
        .filter(test_record::Column::Score.eq(0))
        .all(&db)
        .await
        .unwrap();
    // original score=0 (index 0) + 2 updated = 3 total with score 0
    assert_eq!(zeroed.len(), 3);
}

#[tokio::test]
async fn test_delete_many() {
    let db = setup_db().await;

    for i in 0..5_i32 {
        TestRecord {
            name: format!("del_{i}"),
            score: i,
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
    }

    use modo_db::sea_orm::ColumnTrait;
    let affected = TestRecord::delete_many()
        .filter(test_record::Column::Score.lt(3))
        .exec(&db)
        .await
        .unwrap();

    assert_eq!(affected, 3, "should delete 3 records with score < 3");

    let remaining = TestRecord::find_all(&db).await.unwrap();
    assert_eq!(remaining.len(), 2, "2 records should remain");
    assert!(
        remaining.iter().all(|r| r.score >= 3),
        "remaining records should have score >= 3"
    );
}
