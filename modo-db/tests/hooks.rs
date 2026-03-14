use modo_db::sea_orm::{ConnectionTrait, Database, DatabaseConnection};

// Force inventory registration of test entities
#[allow(unused_imports)]
use audit_item as _;
#[allow(unused_imports)]
use guarded_item as _;
#[allow(unused_imports)]
use hook_item as _;
#[allow(unused_imports)]
use no_hook_item as _;

// -- Entity with a custom before_save hook ------------------------------------

#[modo_db::entity(table = "hook_items")]
#[entity(timestamps)]
pub struct HookItem {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub name: String,
}

// Inherent method — takes priority over DefaultHooks blanket impl
impl HookItem {
    pub fn before_save(&mut self) -> Result<(), modo::Error> {
        self.name = self.name.to_uppercase();
        Ok(())
    }
}

// -- Entity WITHOUT a custom hook (default no-op) -----------------------------

#[modo_db::entity(table = "nohook_items")]
#[entity(timestamps)]
pub struct NoHookItem {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub name: String,
}

// -- Entity with a custom after_save hook -------------------------------------

#[modo_db::entity(table = "audit_items")]
#[entity(timestamps)]
pub struct AuditItem {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub name: String,
}

impl AuditItem {
    pub fn after_save(&self) -> Result<(), modo::Error> {
        // Verify post-condition: name must not be empty after save
        if self.name.is_empty() {
            return Err(modo::Error::from(modo::HttpError::UnprocessableEntity));
        }
        Ok(())
    }
}

// -- Entity with a conditional before_delete hook -----------------------------

#[modo_db::entity(table = "guarded_items")]
#[entity(timestamps)]
pub struct GuardedItem {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub name: String,
    pub locked: bool,
}

impl GuardedItem {
    pub fn before_delete(&self) -> Result<(), modo::Error> {
        if self.locked {
            return Err(modo::Error::from(modo::HttpError::Forbidden));
        }
        Ok(())
    }
}

// -- Setup helper -------------------------------------------------------------

async fn setup_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS hook_items (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .await
    .unwrap();
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS nohook_items (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .await
    .unwrap();
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS audit_items (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .await
    .unwrap();
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS guarded_items (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            locked BOOLEAN NOT NULL DEFAULT 0,
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
async fn test_hook_fires_on_insert() {
    let db = setup_db().await;

    let item = HookItem {
        name: "hello".to_string(),
        ..Default::default()
    };
    let inserted = item.insert(&db).await.unwrap();

    assert_eq!(
        inserted.name, "HELLO",
        "before_save hook should uppercase the name on insert"
    );

    // Verify the stored value is also uppercased
    let found = HookItem::find_by_id(&inserted.id, &db).await.unwrap();
    assert_eq!(found.name, "HELLO");
}

#[tokio::test]
async fn test_hook_fires_on_update() {
    let db = setup_db().await;

    let item = HookItem {
        name: "initial".to_string(),
        ..Default::default()
    };
    let mut item = item.insert(&db).await.unwrap();
    let id = item.id.clone();

    item.name = "world".to_string();
    item.update(&db).await.unwrap();

    assert_eq!(
        item.name, "WORLD",
        "before_save hook should uppercase the name on update"
    );

    let found = HookItem::find_by_id(&id, &db).await.unwrap();
    assert_eq!(found.name, "WORLD");
}

#[tokio::test]
async fn test_no_hook_uses_default_noop() {
    let db = setup_db().await;

    let item = NoHookItem {
        name: "hello".to_string(),
        ..Default::default()
    };
    let inserted = item.insert(&db).await.unwrap();

    assert_eq!(
        inserted.name, "hello",
        "entity without custom hook should leave name unchanged"
    );

    let found = NoHookItem::find_by_id(&inserted.id, &db).await.unwrap();
    assert_eq!(found.name, "hello");
}

#[tokio::test]
async fn test_after_save_fires_on_insert() {
    let db = setup_db().await;

    let item = AuditItem {
        name: "valid_name".to_string(),
        ..Default::default()
    };
    let inserted = item.insert(&db).await.unwrap();

    // after_save should have run without error (name is non-empty)
    assert_eq!(inserted.name, "valid_name");
    let found = AuditItem::find_by_id(&inserted.id, &db).await.unwrap();
    assert_eq!(found.name, "valid_name");
}

#[tokio::test]
async fn test_after_save_fires_on_update() {
    let db = setup_db().await;

    let item = AuditItem {
        name: "original".to_string(),
        ..Default::default()
    };
    let mut item = item.insert(&db).await.unwrap();
    let id = item.id.clone();

    item.name = "updated".to_string();
    item.update(&db).await.unwrap();

    // after_save should have run without error on update
    let found = AuditItem::find_by_id(&id, &db).await.unwrap();
    assert_eq!(found.name, "updated");
}

#[tokio::test]
async fn test_after_save_error_propagates() {
    let db = setup_db().await;

    // Insert with empty name — after_save hook should reject this
    let item = AuditItem {
        name: "".to_string(),
        ..Default::default()
    };
    let result = item.insert(&db).await;

    assert!(
        result.is_err(),
        "after_save should return error when name is empty"
    );
    let err = result.unwrap_err();
    assert_eq!(
        err.status_code(),
        modo::axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "after_save error should propagate with correct status"
    );
}

#[tokio::test]
async fn test_before_delete_allows_unlocked() {
    let db = setup_db().await;

    let item = GuardedItem {
        name: "unlocked_item".to_string(),
        locked: false,
        ..Default::default()
    };
    let inserted = item.insert(&db).await.unwrap();
    let id = inserted.id.clone();

    // Delete unlocked item — before_delete should allow it
    inserted.delete(&db).await.unwrap();

    let result = GuardedItem::find_by_id(&id, &db).await;
    assert!(
        result.is_err(),
        "unlocked item should be deleted successfully"
    );
}

#[tokio::test]
async fn test_before_delete_blocks_locked() {
    let db = setup_db().await;

    let item = GuardedItem {
        name: "locked_item".to_string(),
        locked: true,
        ..Default::default()
    };
    let inserted = item.insert(&db).await.unwrap();
    let id = inserted.id.clone();

    // Delete locked item — before_delete should block it
    let result = inserted.delete(&db).await;

    assert!(
        result.is_err(),
        "before_delete should prevent deleting a locked item"
    );
    let err = result.unwrap_err();
    assert_eq!(
        err.status_code(),
        modo::axum::http::StatusCode::FORBIDDEN,
        "before_delete error should be 403 Forbidden"
    );

    // Record should still exist
    let found = GuardedItem::find_by_id(&id, &db).await.unwrap();
    assert_eq!(found.name, "locked_item");
    assert!(found.locked);
}
