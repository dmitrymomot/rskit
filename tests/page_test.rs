#![cfg(feature = "test-helpers")]

use axum::routing::get;
use modo::page::{
    CursorPage, CursorPaginate, CursorRequest, Page, Paginate, PageRequest, PaginationConfig,
};
use modo::testing::{TestApp, TestDb};

// --- Helpers ---

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
struct Item {
    id: String,
    name: String,
}

async fn setup_db() -> TestDb {
    TestDb::new()
        .await
        .exec(
            "CREATE TABLE items (
                id   TEXT PRIMARY KEY,
                name TEXT NOT NULL
            )",
        )
        .await
}

async fn seed_items(db: &TestDb, count: usize) -> Vec<String> {
    let pool = db.pool();
    let mut ids = Vec::new();
    for i in 1..=count {
        // Zero-padded numbers to simulate ULID lexicographic ordering.
        let id = format!("{i:026}");
        let name = format!("item_{i}");
        sqlx::query("INSERT INTO items (id, name) VALUES (?, ?)")
            .bind(&id)
            .bind(&name)
            .execute(&*pool)
            .await
            .unwrap();
        ids.push(id);
    }
    ids
}

// --- Offset pagination ---

#[tokio::test]
async fn offset_first_page() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    let req = PageRequest { page: 1, per_page: 2 };
    let page: Page<Item> = Paginate::new("SELECT * FROM items ORDER BY id ASC")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id, ids[0]);
    assert_eq!(page.items[1].id, ids[1]);
    assert_eq!(page.total, 5);
    assert_eq!(page.page, 1);
    assert_eq!(page.per_page, 2);
    assert_eq!(page.total_pages, 3);
    assert!(page.has_next);
    assert!(!page.has_prev);
}

#[tokio::test]
async fn offset_middle_page() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    let req = PageRequest { page: 2, per_page: 2 };
    let page: Page<Item> = Paginate::new("SELECT * FROM items ORDER BY id ASC")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id, ids[2]);
    assert_eq!(page.items[1].id, ids[3]);
    assert!(page.has_next);
    assert!(page.has_prev);
}

#[tokio::test]
async fn offset_last_page() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    let req = PageRequest { page: 3, per_page: 2 };
    let page: Page<Item> = Paginate::new("SELECT * FROM items ORDER BY id ASC")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, ids[4]);
    assert!(!page.has_next);
    assert!(page.has_prev);
}

#[tokio::test]
async fn offset_beyond_last_page() {
    let db = setup_db().await;
    seed_items(&db, 3).await;
    let pool = db.read_pool();

    let req = PageRequest { page: 99, per_page: 2 };
    let page: Page<Item> = Paginate::new("SELECT * FROM items ORDER BY id ASC")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert!(page.items.is_empty());
    assert_eq!(page.total, 3);
    assert_eq!(page.total_pages, 2);
}

#[tokio::test]
async fn offset_empty_table() {
    let db = setup_db().await;
    let pool = db.read_pool();

    let req = PageRequest { page: 1, per_page: 20 };
    let page: Page<Item> = Paginate::new("SELECT * FROM items")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert!(page.items.is_empty());
    assert_eq!(page.total, 0);
    assert_eq!(page.total_pages, 0);
    assert!(!page.has_next);
    assert!(!page.has_prev);
}

#[tokio::test]
async fn offset_with_bind_params() {
    let db = setup_db().await;
    seed_items(&db, 5).await;
    sqlx::query("INSERT INTO items (id, name) VALUES (?, ?)")
        .bind("special_001")
        .bind("special")
        .execute(&*db.pool())
        .await
        .unwrap();
    let pool = db.read_pool();

    let req = PageRequest { page: 1, per_page: 10 };
    let page: Page<Item> = Paginate::new("SELECT * FROM items WHERE name = ?")
        .bind("special")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.total, 1);
}

// --- Cursor pagination ---

#[tokio::test]
async fn cursor_first_page_newest_first() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    let req = CursorRequest { after: None, per_page: 2 };
    let page: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 2);
    // Newest first = descending by id
    assert_eq!(page.items[0].id, ids[4]);
    assert_eq!(page.items[1].id, ids[3]);
    assert!(page.has_more);
    assert_eq!(page.next.as_deref(), Some(ids[3].as_str()));
}

#[tokio::test]
async fn cursor_second_page_newest_first() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    // First page
    let req = CursorRequest { after: None, per_page: 2 };
    let page1: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .fetch(&pool, &req)
        .await
        .unwrap();

    // Second page using cursor from first
    let req2 = CursorRequest {
        after: page1.next.clone(),
        per_page: 2,
    };
    let page2: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .fetch(&pool, &req2)
        .await
        .unwrap();

    assert_eq!(page2.items.len(), 2);
    assert_eq!(page2.items[0].id, ids[2]);
    assert_eq!(page2.items[1].id, ids[1]);
    assert!(page2.has_more);
}

#[tokio::test]
async fn cursor_last_page() {
    let db = setup_db().await;
    let ids = seed_items(&db, 3).await;
    let pool = db.read_pool();

    // Request starting after the second item (should get only the first)
    let req = CursorRequest {
        after: Some(ids[1].clone()),
        per_page: 10,
    };
    let page: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, ids[0]);
    assert!(!page.has_more);
    assert!(page.next.is_none());
}

#[tokio::test]
async fn cursor_empty_table() {
    let db = setup_db().await;
    let pool = db.read_pool();

    let req = CursorRequest { after: None, per_page: 20 };
    let page: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert!(page.items.is_empty());
    assert!(!page.has_more);
    assert!(page.next.is_none());
}

#[tokio::test]
async fn cursor_oldest_first() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    let req = CursorRequest { after: None, per_page: 2 };
    let page: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .oldest_first()
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id, ids[0]);
    assert_eq!(page.items[1].id, ids[1]);
    assert!(page.has_more);
    assert_eq!(page.next.as_deref(), Some(ids[1].as_str()));
}

#[tokio::test]
async fn cursor_oldest_first_second_page() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    let req = CursorRequest {
        after: Some(ids[1].clone()),
        per_page: 2,
    };
    let page: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .oldest_first()
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id, ids[2]);
    assert_eq!(page.items[1].id, ids[3]);
    assert!(page.has_more);
}

#[tokio::test]
async fn cursor_with_bind_params() {
    let db = setup_db().await;
    seed_items(&db, 5).await;
    sqlx::query("INSERT INTO items (id, name) VALUES (?, ?)")
        .bind("z_special_001")
        .bind("special")
        .execute(&*db.pool())
        .await
        .unwrap();
    let pool = db.read_pool();

    let req = CursorRequest { after: None, per_page: 10 };
    let page: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items WHERE name = ?")
        .bind("special")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].name, "special");
}

// --- Extractor integration ---

async fn list_items_offset(
    page_req: PageRequest,
    modo::extractor::Service(pool): modo::extractor::Service<modo::db::ReadPool>,
) -> modo::Result<axum::Json<Page<Item>>> {
    let page = Paginate::new("SELECT * FROM items ORDER BY id ASC")
        .fetch(&*pool, &page_req)
        .await?;
    Ok(axum::Json(page))
}

async fn list_items_cursor(
    cursor_req: CursorRequest,
    modo::extractor::Service(pool): modo::extractor::Service<modo::db::ReadPool>,
) -> modo::Result<axum::Json<CursorPage<Item>>> {
    let page = CursorPaginate::new("SELECT * FROM items")
        .fetch(&*pool, &cursor_req)
        .await?;
    Ok(axum::Json(page))
}

#[tokio::test]
async fn extractor_offset_default_params() {
    let db = setup_db().await;
    seed_items(&db, 25).await;

    let app = TestApp::builder()
        .service(db.read_pool())
        .route("/items", get(list_items_offset))
        .build();

    let res = app.get("/items").send().await;
    assert_eq!(res.status(), 200);

    let page: Page<Item> = res.json();
    // Default per_page = 20 (hardcoded fallback)
    assert_eq!(page.items.len(), 20);
    assert_eq!(page.per_page, 20);
    assert_eq!(page.page, 1);
    assert_eq!(page.total, 25);
}

#[tokio::test]
async fn extractor_offset_custom_params() {
    let db = setup_db().await;
    seed_items(&db, 10).await;

    let app = TestApp::builder()
        .service(db.read_pool())
        .route("/items", get(list_items_offset))
        .build();

    let res = app.get("/items?page=2&per_page=3").send().await;
    assert_eq!(res.status(), 200);

    let page: Page<Item> = res.json();
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.page, 2);
    assert_eq!(page.per_page, 3);
}

#[tokio::test]
async fn extractor_offset_per_page_clamped_to_max() {
    let db = setup_db().await;
    seed_items(&db, 5).await;

    let app = TestApp::builder()
        .service(db.read_pool())
        .route("/items", get(list_items_offset))
        .build();

    // Default max is 100, request 999
    let res = app.get("/items?per_page=999").send().await;
    let page: Page<Item> = res.json();
    assert_eq!(page.per_page, 100);
}

#[tokio::test]
async fn extractor_cursor_first_and_next_page() {
    let db = setup_db().await;
    seed_items(&db, 5).await;

    let app = TestApp::builder()
        .service(db.read_pool())
        .route("/items", get(list_items_cursor))
        .build();

    // First page
    let res = app.get("/items?per_page=2").send().await;
    assert_eq!(res.status(), 200);
    let page1: CursorPage<Item> = res.json();
    assert_eq!(page1.items.len(), 2);
    assert!(page1.has_more);

    // Second page
    let next = page1.next.unwrap();
    let res = app
        .get(&format!("/items?after={next}&per_page=2"))
        .send()
        .await;
    let page2: CursorPage<Item> = res.json();
    assert_eq!(page2.items.len(), 2);
    assert!(page2.has_more);

    // No overlap between pages
    let ids1: Vec<_> = page1.items.iter().map(|i| &i.id).collect();
    let ids2: Vec<_> = page2.items.iter().map(|i| &i.id).collect();
    assert!(ids1.iter().all(|id| !ids2.contains(id)));
}

#[tokio::test]
async fn extractor_with_config_from_extensions() {
    let db = setup_db().await;
    seed_items(&db, 50).await;

    let config = PaginationConfig {
        default_per_page: 5,
        max_per_page: 10,
    };

    // Inject config via a middleware layer that adds to extensions
    let config_layer = axum::middleware::from_fn(
        move |mut req: axum::extract::Request, next: axum::middleware::Next| {
            let cfg = config.clone();
            async move {
                req.extensions_mut().insert(cfg);
                next.run(req).await
            }
        },
    );

    let app = TestApp::builder()
        .service(db.read_pool())
        .route("/items", get(list_items_offset))
        .layer(config_layer)
        .build();

    // No per_page specified -- should use config default of 5
    let res = app.get("/items").send().await;
    let page: Page<Item> = res.json();
    assert_eq!(page.per_page, 5);

    // per_page=50 should be clamped to config max of 10
    let res = app.get("/items?per_page=50").send().await;
    let page: Page<Item> = res.json();
    assert_eq!(page.per_page, 10);
}
