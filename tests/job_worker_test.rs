use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use chrono::Utc;
use modo::db;
use modo::error::Result;
use modo::job::{self, Enqueuer, JobOptions, Payload, Worker};
use modo::service::Registry;

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

async fn setup() -> (Registry, db::Pool) {
    let config = {
        let mut c = db::SqliteConfig::default();
        c.path = ":memory:".to_string();
        c
    };
    let pool = db::connect(&config).await.unwrap();
    sqlx::query(CREATE_TABLE).execute(&*pool).await.unwrap();
    sqlx::query(CREATE_INDEX).execute(&*pool).await.unwrap();

    let mut registry = Registry::new();
    let write_pool = db::WritePool::new((*pool).clone());
    registry.add(write_pool);
    registry.add(Enqueuer::new(&pool));
    (registry, pool)
}

fn fast_config() -> job::JobConfig {
    let mut c = job::JobConfig::default();
    c.poll_interval_secs = 0;
    c.stale_threshold_secs = 2;
    c.stale_reaper_interval_secs = 1;
    c.drain_timeout_secs = 5;
    c.queues = vec![{
        let mut q = job::QueueConfig::default();
        q.name = "default".to_string();
        q.concurrency = 2;
        q
    }];
    c.cleanup = None;
    c
}

async fn counting_handler(
    _payload: Payload<serde_json::Value>,
    modo::Service(counter): modo::Service<Arc<AtomicU32>>,
) -> Result<()> {
    counter.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

async fn failing_handler(_payload: Payload<serde_json::Value>) -> Result<()> {
    Err(modo::Error::internal("intentional failure"))
}

#[tokio::test]
async fn worker_processes_enqueued_job() {
    let (mut registry, pool) = setup().await;
    let counter = Arc::new(AtomicU32::new(0));
    registry.add(counter.clone());

    let enqueuer = Enqueuer::new(&pool);
    enqueuer
        .enqueue("count", &serde_json::json!({}))
        .await
        .unwrap();

    let worker = Worker::builder(&fast_config(), &registry)
        .register("count", counting_handler)
        .start()
        .await;

    tokio::time::sleep(Duration::from_millis(500)).await;

    modo::runtime::Task::shutdown(worker).await.unwrap();

    assert_eq!(counter.load(Ordering::SeqCst), 1);

    let (status,): (String,) = sqlx::query_as("SELECT status FROM jobs LIMIT 1")
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(status, "completed");
}

#[tokio::test]
async fn worker_retries_failed_job() {
    let (registry, pool) = setup().await;

    let enqueuer = Enqueuer::new(&pool);
    enqueuer
        .enqueue("fail", &serde_json::json!({}))
        .await
        .unwrap();

    let worker = Worker::builder(&fast_config(), &registry)
        .register_with(
            "fail",
            failing_handler,
            JobOptions {
                max_attempts: 2,
                timeout_secs: 5,
            },
        )
        .start()
        .await;

    // First attempt happens immediately. Retry backoff: min(5 * 2^0, 3600) = 5 seconds.
    // The retried job becomes eligible after 5s, then the poll loop picks it up.
    tokio::time::sleep(Duration::from_secs(8)).await;

    modo::runtime::Task::shutdown(worker).await.unwrap();

    let (status, attempt): (String, i32) =
        sqlx::query_as("SELECT status, attempt FROM jobs LIMIT 1")
            .fetch_one(&*pool)
            .await
            .unwrap();
    assert_eq!(status, "dead");
    assert_eq!(attempt, 2);
}

#[tokio::test]
async fn worker_ignores_unregistered_job_names() {
    let (mut registry, pool) = setup().await;
    let counter = Arc::new(AtomicU32::new(0));
    registry.add(counter.clone());

    let enqueuer = Enqueuer::new(&pool);
    enqueuer
        .enqueue("unknown_job", &serde_json::json!({}))
        .await
        .unwrap();

    let worker = Worker::builder(&fast_config(), &registry)
        .register("other_job", counting_handler)
        .start()
        .await;

    tokio::time::sleep(Duration::from_millis(500)).await;

    modo::runtime::Task::shutdown(worker).await.unwrap();

    // The job should remain pending because the worker only claims jobs
    // whose names match registered handlers (via the IN clause).
    let (status,): (String,) = sqlx::query_as("SELECT status FROM jobs LIMIT 1")
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(status, "pending");
}

#[tokio::test]
async fn worker_shutdown_is_clean() {
    let (registry, _pool) = setup().await;

    async fn noop_handler(_payload: Payload<serde_json::Value>) -> Result<()> {
        Ok(())
    }

    let worker = Worker::builder(&fast_config(), &registry)
        .register("noop", noop_handler)
        .start()
        .await;

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        modo::runtime::Task::shutdown(worker),
    )
    .await;

    assert!(result.is_ok());
    assert!(result.unwrap().is_ok());
}

#[tokio::test]
async fn reaper_resets_stale_running_jobs() {
    let (mut registry, pool) = setup().await;
    let counter = Arc::new(AtomicU32::new(0));
    registry.add(counter.clone());

    // Enqueue a job and manually mark it as stale running
    let enqueuer = Enqueuer::new(&pool);
    let id = enqueuer
        .enqueue("count", &serde_json::json!({}))
        .await
        .unwrap();

    let stale_time = (Utc::now() - chrono::Duration::minutes(2)).to_rfc3339();
    sqlx::query("UPDATE jobs SET status = 'running', attempt = 1, started_at = ? WHERE id = ?")
        .bind(&stale_time)
        .bind(&id)
        .execute(&*pool)
        .await
        .unwrap();

    let config = {
        let mut c = job::JobConfig::default();
        c.poll_interval_secs = 0;
        c.stale_threshold_secs = 1;
        c.stale_reaper_interval_secs = 1;
        c.drain_timeout_secs = 5;
        c.queues = vec![{
            let mut q = job::QueueConfig::default();
            q.name = "default".to_string();
            q.concurrency = 2;
            q
        }];
        c.cleanup = None;
        c
    };

    let worker = Worker::builder(&config, &registry)
        .register("count", counting_handler)
        .start()
        .await;

    // Wait for reaper (1s interval) to reset the stale job,
    // then the poll loop to re-claim and process it
    tokio::time::sleep(Duration::from_secs(3)).await;

    modo::runtime::Task::shutdown(worker).await.unwrap();

    // The reaper should have reset the job to pending,
    // and the worker should have picked it up and completed it
    assert!(counter.load(Ordering::SeqCst) >= 1);

    let (status,): (String,) = sqlx::query_as("SELECT status FROM jobs WHERE id = ?")
        .bind(&id)
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(status, "completed");
}

#[tokio::test]
async fn cleanup_removes_old_terminal_jobs() {
    let (registry, pool) = setup().await;

    // Enqueue a job and manually mark it as completed with old timestamp
    let enqueuer = Enqueuer::new(&pool);
    enqueuer
        .enqueue("test", &serde_json::json!({}))
        .await
        .unwrap();

    let old_time = (Utc::now() - chrono::Duration::minutes(2)).to_rfc3339();
    sqlx::query("UPDATE jobs SET status = 'completed', updated_at = ?")
        .bind(&old_time)
        .execute(&*pool)
        .await
        .unwrap();

    let config = {
        let mut c = job::JobConfig::default();
        c.poll_interval_secs = 60; // high interval — we don't need the poll loop
        c.stale_threshold_secs = 600;
        c.stale_reaper_interval_secs = 600;
        c.drain_timeout_secs = 5;
        c.queues = vec![{
            let mut q = job::QueueConfig::default();
            q.name = "default".to_string();
            q.concurrency = 2;
            q
        }];
        c.cleanup = Some({
            let mut cc = job::CleanupConfig::default();
            cc.interval_secs = 1;
            cc.retention_secs = 1;
            cc
        });
        c
    };

    async fn noop_handler(_payload: Payload<serde_json::Value>) -> Result<()> {
        Ok(())
    }

    let worker = Worker::builder(&config, &registry)
        .register("test", noop_handler)
        .start()
        .await;

    // Wait for cleanup loop (1s interval, 1s retention) to delete the old completed job
    tokio::time::sleep(Duration::from_secs(3)).await;

    modo::runtime::Task::shutdown(worker).await.unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs")
        .fetch_one(&*pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0);
}
