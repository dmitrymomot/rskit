#![cfg(feature = "db")]

use chrono::{Duration, Utc};
use modo::db;
use modo::job::{EnqueueOptions, EnqueueResult, Enqueuer};
use serde::Serialize;

const CREATE_TABLE: &str = "
CREATE TABLE jobs (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    queue         TEXT NOT NULL DEFAULT 'default',
    payload       TEXT NOT NULL DEFAULT '{}',
    payload_hash  TEXT,
    status        TEXT NOT NULL DEFAULT 'pending',
    attempt       INTEGER NOT NULL DEFAULT 0,
    run_at        TEXT NOT NULL,
    started_at    TEXT,
    completed_at  TEXT,
    failed_at     TEXT,
    error_message TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
)";

const CREATE_INDEX: &str = "
CREATE UNIQUE INDEX idx_jobs_payload_hash
    ON jobs(payload_hash)
    WHERE payload_hash IS NOT NULL AND status IN ('pending', 'running')";

async fn setup() -> (Enqueuer, db::Pool) {
    let config = {
        let mut c = db::SqliteConfig::default();
        c.path = ":memory:".to_string();
        c
    };
    let pool = db::connect(&config).await.unwrap();
    sqlx::query(CREATE_TABLE).execute(&*pool).await.unwrap();
    sqlx::query(CREATE_INDEX).execute(&*pool).await.unwrap();
    let enqueuer = Enqueuer::new(&pool);
    (enqueuer, pool)
}

#[derive(Serialize)]
struct TestPayload {
    user_id: String,
}

#[tokio::test]
async fn enqueue_inserts_pending_job() {
    let (enqueuer, pool) = setup().await;
    let id = enqueuer
        .enqueue(
            "send_email",
            &TestPayload {
                user_id: "u1".into(),
            },
        )
        .await
        .unwrap();

    let row: (String, String, String, i64) =
        sqlx::query_as("SELECT name, queue, status, attempt FROM jobs WHERE id = ?")
            .bind(&id)
            .fetch_one(&*pool)
            .await
            .unwrap();

    assert_eq!(row.0, "send_email");
    assert_eq!(row.1, "default");
    assert_eq!(row.2, "pending");
    assert_eq!(row.3, 0);
}

#[tokio::test]
async fn enqueue_at_sets_future_run_at() {
    let (enqueuer, pool) = setup().await;
    let future = Utc::now() + Duration::hours(1);
    let id = enqueuer
        .enqueue_at(
            "report",
            &TestPayload {
                user_id: "u1".into(),
            },
            future,
        )
        .await
        .unwrap();

    let (run_at_str,): (String,) = sqlx::query_as("SELECT run_at FROM jobs WHERE id = ?")
        .bind(&id)
        .fetch_one(&*pool)
        .await
        .unwrap();

    let run_at = chrono::DateTime::parse_from_rfc3339(&run_at_str).unwrap();
    assert!(run_at > Utc::now());
}

#[tokio::test]
async fn enqueue_with_custom_queue() {
    let (enqueuer, pool) = setup().await;
    let id = enqueuer
        .enqueue_with(
            "send_email",
            &TestPayload {
                user_id: "u1".into(),
            },
            EnqueueOptions {
                queue: "email".to_string(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let (queue,): (String,) = sqlx::query_as("SELECT queue FROM jobs WHERE id = ?")
        .bind(&id)
        .fetch_one(&*pool)
        .await
        .unwrap();

    assert_eq!(queue, "email");
}

#[tokio::test]
async fn enqueue_unique_creates_first_time() {
    let (enqueuer, _pool) = setup().await;
    let result = enqueuer
        .enqueue_unique(
            "send_email",
            &TestPayload {
                user_id: "u1".into(),
            },
        )
        .await
        .unwrap();

    assert!(matches!(result, EnqueueResult::Created(_)));
}

#[tokio::test]
async fn enqueue_unique_detects_duplicate() {
    let (enqueuer, _pool) = setup().await;
    let payload = TestPayload {
        user_id: "u1".into(),
    };

    let first = enqueuer
        .enqueue_unique("send_email", &payload)
        .await
        .unwrap();
    let second = enqueuer
        .enqueue_unique("send_email", &payload)
        .await
        .unwrap();

    assert!(matches!(first, EnqueueResult::Created(_)));
    assert!(matches!(second, EnqueueResult::Duplicate(_)));
}

#[tokio::test]
async fn enqueue_unique_allows_different_payload() {
    let (enqueuer, _pool) = setup().await;

    let r1 = enqueuer
        .enqueue_unique(
            "send_email",
            &TestPayload {
                user_id: "u1".into(),
            },
        )
        .await
        .unwrap();
    let r2 = enqueuer
        .enqueue_unique(
            "send_email",
            &TestPayload {
                user_id: "u2".into(),
            },
        )
        .await
        .unwrap();

    assert!(matches!(r1, EnqueueResult::Created(_)));
    assert!(matches!(r2, EnqueueResult::Created(_)));
}

#[tokio::test]
async fn enqueue_unique_with_custom_queue() {
    let (enqueuer, pool) = setup().await;
    let result = enqueuer
        .enqueue_unique_with(
            "send_email",
            &TestPayload {
                user_id: "u1".into(),
            },
            EnqueueOptions {
                queue: "email".to_string(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert!(matches!(result, EnqueueResult::Created(_)));

    let (queue,): (String,) = sqlx::query_as("SELECT queue FROM jobs LIMIT 1")
        .bind(match &result {
            EnqueueResult::Created(id) => id,
            _ => unreachable!(),
        })
        .fetch_one(&*pool)
        .await
        .unwrap();

    assert_eq!(queue, "email");
}

#[tokio::test]
async fn cancel_pending_job_succeeds() {
    let (enqueuer, _pool) = setup().await;
    let id = enqueuer
        .enqueue("test", &serde_json::json!({}))
        .await
        .unwrap();

    let cancelled = enqueuer.cancel(&id).await.unwrap();
    assert!(cancelled);
}

#[tokio::test]
async fn cancel_nonexistent_job_returns_false() {
    let (enqueuer, _pool) = setup().await;
    let cancelled = enqueuer.cancel("nonexistent").await.unwrap();
    assert!(!cancelled);
}

#[tokio::test]
async fn cancel_already_cancelled_returns_false() {
    let (enqueuer, _pool) = setup().await;
    let id = enqueuer
        .enqueue("test", &serde_json::json!({}))
        .await
        .unwrap();

    enqueuer.cancel(&id).await.unwrap();
    let second = enqueuer.cancel(&id).await.unwrap();
    assert!(!second);
}
