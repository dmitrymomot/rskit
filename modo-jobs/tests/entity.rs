use modo_db::EntityRegistration;

// Force the linker to include modo_jobs entity registration.
#[allow(unused_imports)]
use modo_jobs::entity::job as _;

#[test]
fn test_entity_registered() {
    let registrations: Vec<&EntityRegistration> = inventory::iter::<EntityRegistration>().collect();
    let tables: Vec<&str> = registrations.iter().map(|r| r.table_name).collect();
    assert!(
        tables.contains(&"modo_jobs"),
        "modo_jobs not registered. Found: {tables:?}"
    );
}

#[test]
fn test_entity_is_framework() {
    let reg = inventory::iter::<EntityRegistration>()
        .find(|r| r.table_name == "modo_jobs")
        .expect("modo_jobs entity not found");
    assert!(reg.is_framework, "modo_jobs entity should be framework");
}

#[test]
fn test_entity_group_is_jobs() {
    let reg = inventory::iter::<EntityRegistration>()
        .find(|r| r.table_name == "modo_jobs")
        .expect("modo_jobs entity not found");
    assert_eq!(
        reg.group, "jobs",
        "modo_jobs entity should be in 'jobs' group"
    );
}

#[test]
fn test_entity_has_claim_index() {
    let reg = inventory::iter::<EntityRegistration>()
        .find(|r| r.table_name == "modo_jobs")
        .expect("modo_jobs entity not found");
    assert!(
        reg.extra_sql
            .iter()
            .any(|s| s.contains("idx_modo_jobs_state_queue_run_at_priority")),
        "Expected claim index in extra_sql"
    );
}

#[tokio::test]
async fn test_entity_table_created() {
    use modo_db::sea_orm::{ConnectionTrait, Database, Schema};

    let db = Database::connect("sqlite::memory:")
        .await
        .expect("Failed to connect to in-memory SQLite");

    // Sync schema
    let schema = Schema::new(db.get_database_backend());
    let mut builder = schema.builder();
    builder = (inventory::iter::<EntityRegistration>()
        .find(|r| r.table_name == "modo_jobs")
        .unwrap()
        .register_fn)(builder);
    builder.sync(&db).await.expect("Schema sync failed");

    // Execute extra SQL
    let reg = inventory::iter::<EntityRegistration>()
        .find(|r| r.table_name == "modo_jobs")
        .unwrap();
    for sql in reg.extra_sql {
        db.execute_unprepared(sql).await.expect("Extra SQL failed");
    }

    // Verify table exists by querying
    let result = db
        .execute_unprepared("SELECT COUNT(*) FROM modo_jobs")
        .await;
    assert!(result.is_ok(), "modo_jobs table should exist");
}

#[tokio::test]
async fn test_claim_index_created() {
    use modo_db::sea_orm::{ConnectionTrait, Database, Schema};

    let db = Database::connect("sqlite::memory:")
        .await
        .expect("Failed to connect");

    let schema = Schema::new(db.get_database_backend());
    let mut builder = schema.builder();
    let reg = inventory::iter::<EntityRegistration>()
        .find(|r| r.table_name == "modo_jobs")
        .unwrap();
    builder = (reg.register_fn)(builder);
    builder.sync(&db).await.expect("Schema sync failed");
    for sql in reg.extra_sql {
        db.execute_unprepared(sql).await.expect("Extra SQL failed");
    }

    // Check sqlite_master for the index
    let result = db
        .execute_unprepared(
            "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_modo_jobs_state_queue_run_at_priority'",
        )
        .await;
    assert!(result.is_ok(), "claim index should exist");
}
