# modo-jobs

[![docs.rs](https://img.shields.io/docsrs/modo-jobs)](https://docs.rs/modo-jobs)

Database-backed background job processing for the modo framework.

Jobs are defined as plain async functions, registered at link time via the
`#[job]` attribute macro, and executed by a multi-queue worker pool. No manual
registration call is needed at startup.

## Key Features

- Compile-time job registration via `#[job]`
- Per-queue concurrency limits
- Automatic retries with exponential backoff (5s × 2^(attempt-1), capped at 1h)
- Scheduled execution via `enqueue_at`
- In-memory cron scheduling (not persisted to the database)
- Graceful shutdown with configurable drain timeout
- Supported databases: SQLite, PostgreSQL

## Usage

### Defining Jobs

```rust
use modo_jobs::job;
use modo::HandlerResult;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct WelcomePayload {
    email: String,
}

#[job(queue = "default", max_attempts = 5, timeout = "30s")]
async fn send_welcome(payload: WelcomePayload) -> HandlerResult<()> {
    tracing::info!(email = %payload.email, "Sending welcome email");
    Ok(())
}
```

The macro generates a `SendWelcomeJob` struct with `enqueue` and `enqueue_at`
associated functions.

### Injecting Services and the Database

Use `Service<T>` to inject a registered service and `Db` to inject the database
pool directly in the job function signature:

```rust
use modo_jobs::job;
use modo::HandlerResult;
use modo::extractor::Service;
use modo_db::extractor::Db;

#[job(queue = "default")]
async fn process_report(Db(db): Db, Service(mailer): Service<MyMailer>) -> HandlerResult<()> {
    // db: DbPool, mailer: Arc<MyMailer>
    Ok(())
}
```

Services must be registered with `JobsBuilder::service(...)` before calling `.run()`.

### Cron Jobs

```rust
use modo_jobs::job;
use modo::HandlerResult;

#[job(cron = "0 */5 * * * *", timeout = "10s")]
async fn heartbeat() -> HandlerResult<()> {
    tracing::info!("heartbeat tick");
    Ok(())
}
```

Cron jobs run in-memory only and are not persisted to the database. They cannot
have `queue`, `priority`, or `max_attempts` attributes. At most one instance of
each cron job runs at a time — if execution takes longer than the interval, the
next tick is skipped.

### Starting the Runner

```rust
#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = modo_db::connect(&config.database).await?;
    modo_db::sync_and_migrate(&db).await?;

    let jobs = modo_jobs::new(&db, &config.jobs)
        .service(db.clone())
        .run()
        .await?;

    app.config(config.core)
        .managed_service(db)
        .managed_service(jobs)
        .run()
        .await
}
```

`managed_service(jobs)` registers `JobsHandle` for graceful shutdown in the
`Drain` phase.

#### Separate database for jobs (SQLite)

The `modo_jobs::entity::job` entity uses `group = "jobs"`, allowing it to be
synced to a separate database. This is useful with SQLite where concurrent
writes to a single database can cause lock contention:

```rust
let db = modo_db::connect(&config.database).await?;

// Sync jobs entity to a separate SQLite database
let jobs_db = modo_db::connect(&config.jobs_database).await?;
modo_db::sync_and_migrate_group(&jobs_db, "jobs").await?;
modo_db::sync_and_migrate(&db).await?;

let jobs = modo_jobs::new(&jobs_db, &config.jobs)
    .service(db.clone())
    .run()
    .await?;
```

### Enqueuing from HTTP Handlers

`JobQueue` is an axum extractor that resolves from the registered `JobsHandle`.

```rust
use modo::extractor::JsonReq;
use modo::{Json, JsonResult};
use modo_jobs::JobQueue;
use serde_json::{json, Value};

#[modo::handler(POST, "/welcome")]
async fn enqueue_welcome(
    queue: JobQueue,
    input: JsonReq<WelcomePayload>,
) -> JsonResult<Value> {
    let job_id = SendWelcomeJob::enqueue(&queue, &input).await?;
    Ok(Json(json!({ "job_id": job_id.to_string() })))
}
```

### Scheduled Enqueue

```rust
let run_at = chrono::Utc::now() + chrono::Duration::seconds(60);
let job_id = SendWelcomeJob::enqueue_at(&queue, &payload, run_at).await?;
```

### Cancelling a Job

```rust
queue.cancel(&job_id).await?;
```

Only jobs in the `Pending` state can be cancelled.

## Configuration

`JobsConfig` can be deserialized from YAML:

```yaml
poll_interval_secs: 1          # how often each queue polls (default: 1)
stale_threshold_secs: 600      # re-queue jobs locked longer than this (default: 600)
stale_reaper_interval_secs: 60 # how often the stale reaper runs (default: 60)
drain_timeout_secs: 30         # max wait during shutdown (default: 30)
max_payload_bytes: null        # payload size limit, null = unlimited (default: null)
max_queue_depth: null          # max pending jobs per queue, null = unlimited (default: null)

queues:
  - name: default
    concurrency: 4
  - name: emails
    concurrency: 2

cleanup:
  interval_secs: 3600     # how often cleanup runs (default: 3600)
  retention_secs: 86400   # delete finished jobs older than this (default: 86400)
  statuses: [completed, dead, cancelled]
```

Queue names in YAML must match the `queue` attribute used in `#[job(queue = "...")]`.

When `max_queue_depth` is set and a queue is full, `enqueue()` returns a 503
error. The depth check is a soft limit — concurrent enqueues may briefly exceed
the cap because the count and insert are not atomic.

## Key Types

| Type              | Description                                                              |
| ----------------- | ------------------------------------------------------------------------ |
| `JobsConfig`      | Top-level configuration struct                                           |
| `QueueConfig`     | Per-queue name and concurrency settings                                  |
| `CleanupConfig`   | Finished-job retention policy                                            |
| `JobQueue`        | Enqueue and cancel jobs; axum extractor                                  |
| `JobsHandle`      | Runner handle; derefs to `JobQueue`; implements `GracefulShutdown`       |
| `JobContext`      | Passed to each handler — provides payload, services, and DB access       |
| `JobHandler`      | Trait implemented by `#[job]`-generated structs                          |
| `JobRegistration` | Compile-time registration record collected via `inventory`               |
| `JobId`           | ULID-backed unique job identifier                                        |
| `JobState`        | `Pending`, `Running`, `Completed`, `Dead`, `Cancelled`                   |

## `#[job]` Macro Parameters

| Parameter      | Type                      | Default     | Notes                                                       |
| -------------- | ------------------------- | ----------- | ----------------------------------------------------------- |
| `queue`        | string                    | `"default"` | Must match a configured queue                               |
| `priority`     | integer                   | `0`         | Higher = runs sooner within the queue                       |
| `max_attempts` | integer                   | `3`         | Retries before `dead`                                       |
| `timeout`      | `"Xs"` / `"Xm"` / `"Xh"` | `"5m"`      | Per-execution timeout                                       |
| `cron`         | cron expression           | —           | Mutually exclusive with `queue`, `priority`, `max_attempts` |

## Database Schema

The `modo_jobs` table is created and migrated automatically by
`modo_db::sync_and_migrate` (or `modo_db::sync_and_migrate_group(db, "jobs")`
when using a separate jobs database). A composite index on
`(state, queue, run_at, priority)` supports efficient atomic job claiming.
