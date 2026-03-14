use modo_db::Record;
use modo_db::sea_orm::{ConnectionTrait, Database, DatabaseConnection};

// Force inventory registration of test entity
#[allow(unused_imports)]
use soft_item as _;

// -- Test entity with soft-delete ---------------------------------------------

#[modo_db::entity(table = "soft_items")]
#[entity(timestamps, soft_delete)]
pub struct SoftItem {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub name: String,
}

// -- Setup helper -------------------------------------------------------------

async fn setup_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS soft_items (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT
        )",
    )
    .await
    .unwrap();
    db
}

// -- Tests --------------------------------------------------------------------

#[tokio::test]
async fn test_delete_sets_deleted_at() {
    let db = setup_db().await;

    let item = SoftItem {
        name: "to_soft_delete".to_string(),
        ..Default::default()
    };
    let inserted = item.insert(&db).await.unwrap();
    let id = inserted.id.clone();

    assert!(
        inserted.deleted_at.is_none(),
        "deleted_at should be None before deletion"
    );

    inserted.delete(&db).await.unwrap();

    // Retrieve via with_deleted to check deleted_at is now set
    use modo_db::sea_orm::ColumnTrait;
    let found = SoftItem::with_deleted()
        .filter(soft_item::Column::Id.eq(&id))
        .one(&db)
        .await
        .unwrap()
        .expect("record should still exist via with_deleted");

    assert!(
        found.deleted_at.is_some(),
        "deleted_at should be set after soft-delete"
    );
}

#[tokio::test]
async fn test_find_all_excludes_deleted() {
    let db = setup_db().await;

    let keep = SoftItem {
        name: "keep".to_string(),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let remove = SoftItem {
        name: "remove".to_string(),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    remove.delete(&db).await.unwrap();

    let all = SoftItem::find_all(&db).await.unwrap();
    assert_eq!(all.len(), 1, "find_all should exclude soft-deleted records");
    assert_eq!(all[0].id, keep.id);
}

#[tokio::test]
async fn test_find_by_id_excludes_deleted() {
    let db = setup_db().await;

    let item = SoftItem {
        name: "gone".to_string(),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let id = item.id.clone();

    item.delete(&db).await.unwrap();

    let result = SoftItem::find_by_id(&id, &db).await;
    assert!(
        result.is_err(),
        "find_by_id should return error for soft-deleted record"
    );
    let err = result.unwrap_err();
    assert_eq!(
        err.status_code(),
        modo::axum::http::StatusCode::NOT_FOUND,
        "error should be 404 Not Found"
    );
}

#[tokio::test]
async fn test_query_excludes_deleted() {
    let db = setup_db().await;

    let visible = SoftItem {
        name: "visible".to_string(),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let hidden = SoftItem {
        name: "hidden".to_string(),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    hidden.delete(&db).await.unwrap();

    let results = SoftItem::query().all(&db).await.unwrap();
    assert_eq!(
        results.len(),
        1,
        "query() should exclude soft-deleted records"
    );
    assert_eq!(results[0].id, visible.id);
}

#[tokio::test]
async fn test_with_deleted_includes_all() {
    let db = setup_db().await;

    let item_a = SoftItem {
        name: "alpha".to_string(),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let item_b = SoftItem {
        name: "beta".to_string(),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    item_b.delete(&db).await.unwrap();

    let all = SoftItem::with_deleted().all(&db).await.unwrap();
    assert_eq!(
        all.len(),
        2,
        "with_deleted() should include both active and soft-deleted records"
    );

    let ids: Vec<&str> = all.iter().map(|i| i.id.as_str()).collect();
    assert!(ids.contains(&item_a.id.as_str()));
}

#[tokio::test]
async fn test_only_deleted_returns_deleted_only() {
    let db = setup_db().await;

    SoftItem {
        name: "active".to_string(),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let deleted = SoftItem {
        name: "deleted".to_string(),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let deleted_id = deleted.id.clone();

    deleted.delete(&db).await.unwrap();

    let only_deleted = SoftItem::only_deleted().all(&db).await.unwrap();
    assert_eq!(
        only_deleted.len(),
        1,
        "only_deleted() should return exactly one deleted record"
    );
    assert_eq!(only_deleted[0].id, deleted_id);
    assert!(
        only_deleted[0].deleted_at.is_some(),
        "returned record should have deleted_at set"
    );
}

#[tokio::test]
async fn test_restore() {
    let db = setup_db().await;

    let item = SoftItem {
        name: "to_restore".to_string(),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let id = item.id.clone();

    item.delete(&db).await.unwrap();

    // Confirm it's gone from normal queries
    let result = SoftItem::find_by_id(&id, &db).await;
    assert!(result.is_err(), "should not be found after soft-delete");

    // Retrieve via with_deleted, then restore
    use modo_db::sea_orm::ColumnTrait;
    let mut soft_deleted = SoftItem::with_deleted()
        .filter(soft_item::Column::Id.eq(&id))
        .one(&db)
        .await
        .unwrap()
        .expect("should be retrievable via with_deleted");

    soft_deleted.restore(&db).await.unwrap();

    // Now find_by_id should succeed
    let restored = SoftItem::find_by_id(&id, &db).await.unwrap();
    assert_eq!(restored.id, id);
    assert!(
        restored.deleted_at.is_none(),
        "deleted_at should be cleared after restore"
    );
}

#[tokio::test]
async fn test_restore_on_active_record() {
    let db = setup_db().await;

    let item = SoftItem {
        name: "active_item".to_string(),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let id = item.id.clone();

    // Record is not deleted — deleted_at should be None
    assert!(item.deleted_at.is_none());

    // Restore on non-deleted record should be a no-op
    let mut active = SoftItem::find_by_id(&id, &db).await.unwrap();
    active.restore(&db).await.unwrap();

    // Record should still be accessible and unchanged
    let found = SoftItem::find_by_id(&id, &db).await.unwrap();
    assert_eq!(found.id, id);
    assert!(found.deleted_at.is_none());
    assert_eq!(found.name, "active_item");
}

#[tokio::test]
async fn test_force_delete_removes_completely() {
    let db = setup_db().await;

    let item = SoftItem {
        name: "force_gone".to_string(),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let id = item.id.clone();

    item.force_delete(&db).await.unwrap();

    // Should not appear even with with_deleted
    use modo_db::sea_orm::ColumnTrait;
    let result = SoftItem::with_deleted()
        .filter(soft_item::Column::Id.eq(&id))
        .one(&db)
        .await
        .unwrap();

    assert!(
        result.is_none(),
        "force_delete should remove the record completely, not appear even with with_deleted()"
    );
}
