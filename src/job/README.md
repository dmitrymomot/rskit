# job

Durable background job processing backed by SQLite.

Requires feature `"job"` (implies `"db"`):

```toml
[dependencies]
modo = { version = "0.6", features = ["job"] }
```

## Overview

The `job` module provides a named-job queue stored in the `jobs` SQLite table.
Handlers are plain `async fn` functions. The worker polls the database, dispatches
jobs to handlers, retries failures with exponential backoff, and reaps stale jobs.

End-applications own the `jobs` table migration — this module ships none.

## Key types

| Type             | Role                                                                         |
| ---------------- | ---------------------------------------------------------------------------- |
| `JobConfig`      | Worker configuration (poll interval, queues, cleanup)                        |
| `QueueConfig`    | Name and concurrency limit for a single queue                                |
| `CleanupConfig`  | Interval and retention window for terminal-job cleanup                       |
| `Enqueuer`       | Inserts and cancels jobs in the database                                     |
| `EnqueueOptions` | Queue name and optional scheduled `run_at` timestamp                         |
| `EnqueueResult`  | `Created(id)` or `Duplicate(id)` from idempotent enqueue                     |
| `Worker`         | Running worker handle; implements `Task` for graceful shutdown               |
| `WorkerBuilder`  | Fluent builder for registering handlers and starting the worker              |
| `JobOptions`     | Per-handler max-attempts and timeout                                         |
| `Payload<T>`     | Handler argument — deserializes the JSON payload into `T`                    |
| `Meta`           | Handler argument — job ID, name, queue, attempt count, deadline              |
| `Status`         | Job lifecycle status: `Pending`, `Running`, `Completed`, `Dead`, `Cancelled` |
| `JobHandler`     | Trait blanket-implemented for plain `async fn`s with 0–12 args               |
| `JobContext`     | Runtime context passed to handlers; carries payload, metadata, and registry  |
| `FromJobContext` | Extraction trait for custom handler argument types                           |

## Configuration (YAML)

```yaml
job:
    poll_interval_secs: 1
    stale_threshold_secs: 600
    stale_reaper_interval_secs: 60
    drain_timeout_secs: 30
    queues:
        - name: default
          concurrency: 4
        - name: critical
          concurrency: 2
    cleanup:
        interval_secs: 3600
        retention_secs: 259200 # 72 hours
    # Optional: use a separate SQLite database for the job queue
    # database:
    #     path: data/jobs.db
```

Set `cleanup: ~` (null) to disable automatic cleanup of terminal jobs.

Set `database` to use a separate SQLite file for job-queue writes, keeping them
from contending with application queries.

## Usage

### Defining and registering handlers

```rust,ignore
use modo::job::{JobConfig, JobOptions, Payload, Meta, Worker};
use modo::service::Registry;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct WelcomePayload { user_id: String }

async fn send_welcome_email(
    payload: Payload<WelcomePayload>,
    meta: Meta,
) -> modo::Result<()> {
    tracing::info!(job_id = %meta.id, user_id = %payload.user_id, "sending welcome email");
    Ok(())
}

async fn start_worker(config: &JobConfig, registry: &Registry) {
    let worker = Worker::builder(config, registry)
        .register("send_welcome_email", send_welcome_email)
        .register_with(
            "send_welcome_email_retry1",
            send_welcome_email,
            JobOptions { max_attempts: 1, timeout_secs: 60 },
        )
        .start()
        .await;

    // Integrate with graceful shutdown:
    // modo::run!(server, worker).await.unwrap();
}
```

`Worker::builder` panics if a `Database` is not registered in the registry.

### Enqueueing jobs

```rust,ignore
use modo::job::{Enqueuer, EnqueueOptions, EnqueueResult};
use modo::db::Database;
use serde::Serialize;
use chrono::Utc;

#[derive(Serialize)]
struct WelcomePayload { user_id: String }

async fn enqueue_jobs(db: Database) {
    let enqueuer = Enqueuer::new(db);

    // Immediate execution on default queue
    let id = enqueuer.enqueue("send_welcome_email", &WelcomePayload {
        user_id: "usr_01".into(),
    }).await.unwrap();

    // Scheduled execution
    let run_at = Utc::now() + chrono::Duration::minutes(5);
    enqueuer.enqueue_at("send_welcome_email", &WelcomePayload {
        user_id: "usr_02".into(),
    }, run_at).await.unwrap();

    // Named queue with full options
    enqueuer.enqueue_with("send_welcome_email", &WelcomePayload {
        user_id: "usr_03".into(),
    }, EnqueueOptions { queue: "critical".into(), run_at: None }).await.unwrap();

    // Idempotent — returns Duplicate if a matching job is already pending/running
    match enqueuer.enqueue_unique("send_welcome_email", &WelcomePayload {
        user_id: "usr_01".into(),
    }).await.unwrap() {
        EnqueueResult::Created(id) => println!("new job: {id}"),
        EnqueueResult::Duplicate(id) => println!("already queued: {id}"),
    }

    // Cancel a pending job
    enqueuer.cancel(&id).await.unwrap();
}
```

### Accessing services inside handlers

Use `Service<T>` as a handler argument to retrieve a value registered in the service
registry. The inner value is `Arc<T>`.

```rust,ignore
use modo::job::Payload;
use modo::Service;
use std::sync::Arc;
use serde::Deserialize;

struct Mailer;

#[derive(Deserialize)]
struct WelcomePayload { user_id: String }

async fn notify(
    payload: Payload<WelcomePayload>,
    mailer: Service<Mailer>,
) -> modo::Result<()> {
    let _mailer: Arc<Mailer> = mailer.0.clone();
    Ok(())
}
```

## Retry Behaviour

Failed jobs are rescheduled with exponential backoff: `delay = min(5 * 2^(attempt-1), 3600)` seconds.
After `max_attempts` failures (default 3) the job is moved to `Dead` and not retried.

## Database Schema

The module reads and writes a `jobs` table. End-applications must create and
migrate this table themselves — no embedded migration is provided.

```sql
CREATE TABLE IF NOT EXISTS jobs (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    queue         TEXT NOT NULL DEFAULT 'default',
    payload       TEXT NOT NULL,
    payload_hash  TEXT,
    status        TEXT NOT NULL DEFAULT 'pending',
    attempt       INTEGER NOT NULL DEFAULT 0,
    run_at        TEXT NOT NULL,
    error_message TEXT,
    started_at    TEXT,
    completed_at  TEXT,
    failed_at     TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

-- Poll loop: claims pending jobs by queue and handler name, ordered by run_at
CREATE INDEX IF NOT EXISTS idx_jobs_poll
    ON jobs (status, queue, run_at)
    WHERE status = 'pending';

-- Stale reaper: finds running jobs older than the threshold
CREATE INDEX IF NOT EXISTS idx_jobs_reaper
    ON jobs (status, started_at)
    WHERE status = 'running';

-- Cleanup: deletes terminal jobs past the retention window
CREATE INDEX IF NOT EXISTS idx_jobs_cleanup
    ON jobs (status, updated_at)
    WHERE status IN ('completed', 'dead', 'cancelled');

-- Idempotent enqueue: enforces one active job per payload hash
CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_payload_hash
    ON jobs (payload_hash)
    WHERE payload_hash IS NOT NULL
      AND status IN ('pending', 'running');
```

| Column          | Type      | Description                                                      |
| --------------- | --------- | ---------------------------------------------------------------- |
| `id`            | `TEXT PK` | ULID generated by `id::ulid()`                                   |
| `name`          | `TEXT`    | Registered handler name (e.g. `"send_welcome_email"`)            |
| `queue`         | `TEXT`    | Queue name, defaults to `"default"`                              |
| `payload`       | `TEXT`    | JSON-serialized job arguments                                    |
| `payload_hash`  | `TEXT`    | SHA-256 of `name + "\0" + payload`; set only by `enqueue_unique` |
| `status`        | `TEXT`    | One of `pending`, `running`, `completed`, `dead`, `cancelled`    |
| `attempt`       | `INTEGER` | Execution count; starts at `0`, incremented on each claim        |
| `run_at`        | `TEXT`    | RFC 3339 timestamp — job is not eligible until this time         |
| `error_message` | `TEXT`    | Last failure message (handler error or `"timeout"`)              |
| `started_at`    | `TEXT`    | Set when the worker claims the job; cleared on retry             |
| `completed_at`  | `TEXT`    | Set on successful completion                                     |
| `failed_at`     | `TEXT`    | Set on each failure (retained across retries)                    |
| `created_at`    | `TEXT`    | Insertion timestamp                                              |
| `updated_at`    | `TEXT`    | Last modification timestamp                                      |
