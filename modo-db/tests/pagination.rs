use modo_db::sea_orm::{ConnectionTrait, Database, DatabaseConnection, EntityTrait};
use modo_db::{CursorParams, CursorResult, PageParams, PageResult, paginate, paginate_cursor};

// Force inventory registration of the test entity
#[allow(unused_imports)]
use pag_item as _;

// -- Test entity definition ---------------------------------------------------

#[modo_db::entity(table = "pag_items")]
#[entity(timestamps)]
pub struct PagItem {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub title: String,
    #[entity(default_value = 0)]
    pub position: i32,
}

// -- Setup helpers ------------------------------------------------------------

async fn setup_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS pag_items (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .await
    .unwrap();
    db
}

async fn seed(db: &DatabaseConnection, count: usize) -> Vec<PagItem> {
    let mut items = Vec::new();
    for i in 0..count {
        let item = PagItem {
            title: format!("item-{i:03}"),
            position: i as i32,
            ..Default::default()
        };
        let inserted = item.insert(db).await.unwrap();
        items.push(inserted);
        // Small delay to ensure ULIDs are ordered
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    items
}

// -- Offset pagination tests --------------------------------------------------

#[tokio::test]
async fn offset_first_page() {
    let db = setup_db().await;
    seed(&db, 25).await;

    let params = PageParams {
        page: 1,
        per_page: 10,
    };
    let result: PageResult<pag_item::Model> = paginate(pag_item::Entity::find(), &db, &params)
        .await
        .unwrap();

    assert_eq!(result.data.len(), 10);
    assert!(result.has_next, "first page of 25 items should have next");
    assert!(!result.has_prev, "first page should not have prev");
}

#[tokio::test]
async fn offset_middle_page() {
    let db = setup_db().await;
    seed(&db, 25).await;

    let params = PageParams {
        page: 2,
        per_page: 10,
    };
    let result: PageResult<pag_item::Model> = paginate(pag_item::Entity::find(), &db, &params)
        .await
        .unwrap();

    assert_eq!(result.data.len(), 10);
    assert!(result.has_next, "middle page should have next");
    assert!(result.has_prev, "middle page should have prev");
}

#[tokio::test]
async fn offset_last_page() {
    let db = setup_db().await;
    seed(&db, 25).await;

    let params = PageParams {
        page: 3,
        per_page: 10,
    };
    let result: PageResult<pag_item::Model> = paginate(pag_item::Entity::find(), &db, &params)
        .await
        .unwrap();

    assert_eq!(result.data.len(), 5);
    assert!(!result.has_next, "last page should not have next");
    assert!(result.has_prev, "last page should have prev");
}

#[tokio::test]
async fn offset_beyond_last_page() {
    let db = setup_db().await;
    seed(&db, 5).await;

    let params = PageParams {
        page: 10,
        per_page: 10,
    };
    let result: PageResult<pag_item::Model> = paginate(pag_item::Entity::find(), &db, &params)
        .await
        .unwrap();

    assert!(result.data.is_empty(), "page beyond last should be empty");
    assert!(!result.has_next, "page beyond last should not have next");
    assert!(result.has_prev, "page beyond last should have prev");
}

#[tokio::test]
async fn offset_per_page_clamped_to_100() {
    let db = setup_db().await;

    let params = PageParams {
        page: 1,
        per_page: 999,
    };
    let result: PageResult<pag_item::Model> = paginate(pag_item::Entity::find(), &db, &params)
        .await
        .unwrap();

    assert_eq!(result.per_page, 100, "per_page should be clamped to 100");
}

#[tokio::test]
async fn offset_page_zero_treated_as_one() {
    let db = setup_db().await;
    seed(&db, 5).await;

    let params = PageParams {
        page: 0,
        per_page: 10,
    };
    let result: PageResult<pag_item::Model> = paginate(pag_item::Entity::find(), &db, &params)
        .await
        .unwrap();

    assert_eq!(result.page, 1, "page 0 should be treated as page 1");
    assert_eq!(result.data.len(), 5);
    assert!(!result.has_prev, "page 1 should not have prev");
}

// -- Cursor pagination tests --------------------------------------------------

#[tokio::test]
async fn cursor_first_page() {
    let db = setup_db().await;
    seed(&db, 15).await;

    let params = CursorParams::<String> {
        per_page: Some(5),
        ..Default::default()
    };
    let result: CursorResult<pag_item::Model> = paginate_cursor(
        pag_item::Entity::find(),
        pag_item::Column::Id,
        |m: &pag_item::Model| m.id.clone(),
        &db,
        &params,
    )
    .await
    .unwrap();

    assert_eq!(result.data.len(), 5);
    assert!(
        result.has_next,
        "first cursor page of 15 items should have next"
    );
    assert!(!result.has_prev, "first cursor page should not have prev");
    assert!(
        result.next_cursor.is_some(),
        "first cursor page should provide next_cursor"
    );
}

#[tokio::test]
async fn cursor_forward_navigation() {
    let db = setup_db().await;
    seed(&db, 15).await;

    // First page
    let params1 = CursorParams::<String> {
        per_page: Some(5),
        ..Default::default()
    };
    let page1: CursorResult<pag_item::Model> = paginate_cursor(
        pag_item::Entity::find(),
        pag_item::Column::Id,
        |m: &pag_item::Model| m.id.clone(),
        &db,
        &params1,
    )
    .await
    .unwrap();

    let next_cursor = page1.next_cursor.expect("page1 should have next_cursor");

    // Second page using next_cursor
    let params2 = CursorParams::<String> {
        per_page: Some(5),
        after: Some(next_cursor),
        ..Default::default()
    };
    let page2: CursorResult<pag_item::Model> = paginate_cursor(
        pag_item::Entity::find(),
        pag_item::Column::Id,
        |m: &pag_item::Model| m.id.clone(),
        &db,
        &params2,
    )
    .await
    .unwrap();

    assert_eq!(page2.data.len(), 5);

    // Ensure no overlap between page1 and page2
    let page1_ids: Vec<&str> = page1.data.iter().map(|m| m.id.as_str()).collect();
    let page2_ids: Vec<&str> = page2.data.iter().map(|m| m.id.as_str()).collect();
    for id in &page2_ids {
        assert!(
            !page1_ids.contains(id),
            "page2 should not overlap with page1, but found {id} in both"
        );
    }
}

#[tokio::test]
async fn cursor_last_page_no_next() {
    let db = setup_db().await;
    seed(&db, 7).await;

    // First page: 5 items
    let params1 = CursorParams::<String> {
        per_page: Some(5),
        ..Default::default()
    };
    let page1: CursorResult<pag_item::Model> = paginate_cursor(
        pag_item::Entity::find(),
        pag_item::Column::Id,
        |m: &pag_item::Model| m.id.clone(),
        &db,
        &params1,
    )
    .await
    .unwrap();

    let next_cursor = page1.next_cursor.expect("page1 should have next_cursor");

    // Second (last) page: 2 items
    let params2 = CursorParams::<String> {
        per_page: Some(5),
        after: Some(next_cursor),
        ..Default::default()
    };
    let page2: CursorResult<pag_item::Model> = paginate_cursor(
        pag_item::Entity::find(),
        pag_item::Column::Id,
        |m: &pag_item::Model| m.id.clone(),
        &db,
        &params2,
    )
    .await
    .unwrap();

    assert_eq!(
        page2.data.len(),
        2,
        "last page should have 2 remaining items"
    );
    assert!(!page2.has_next, "last page should not have next");
    assert!(page2.has_prev, "last page should have prev");
}
