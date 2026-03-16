use modo_db::Record;
use modo_db::sea_orm::{ConnectionTrait, Database, DatabaseConnection};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::task::JoinSet;

// Force inventory registration
#[allow(unused_imports)]
use stress_item as _;

#[modo_db::entity(table = "stress_items")]
#[entity(timestamps)]
pub struct StressItem {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub label: String,
    #[entity(default_value = 0)]
    pub counter: i32,
}

async fn setup_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS stress_items (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            counter INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .await
    .unwrap();
    db
}

#[tokio::test]
async fn concurrent_inserts_no_corruption() {
    let db = Arc::new(setup_db().await);

    let mut set = JoinSet::new();

    for task_num in 0..20 {
        let db = db.clone();
        set.spawn(async move {
            let mut ids = Vec::new();
            for i in 0..10 {
                let item = StressItem {
                    label: format!("task{task_num}_item{i}"),
                    ..Default::default()
                };
                let inserted = item.insert(&*db).await.unwrap();
                ids.push(inserted.id.clone());
            }
            ids
        });
    }

    let mut all_ids = Vec::new();
    while let Some(result) = set.join_next().await {
        let ids = result.expect("Task panicked");
        all_ids.extend(ids);
    }

    // All 200 inserts should have completed
    assert_eq!(
        all_ids.len(),
        200,
        "Expected 200 inserts, got {}",
        all_ids.len()
    );

    // All IDs should be unique (ULIDs should never collide)
    let unique_ids: HashSet<&String> = all_ids.iter().collect();
    assert_eq!(
        unique_ids.len(),
        200,
        "Expected 200 unique IDs, got {}",
        unique_ids.len()
    );

    // Verify via find_all that all rows are in the database
    let all_rows = StressItem::find_all(&*db).await.unwrap();
    assert_eq!(
        all_rows.len(),
        200,
        "find_all should return 200 rows, got {}",
        all_rows.len()
    );

    // Verify all returned IDs match our inserts
    let db_ids: HashSet<String> = all_rows.into_iter().map(|r| r.id).collect();
    for id in &all_ids {
        assert!(db_ids.contains(id), "DB should contain inserted ID {id}");
    }
}
