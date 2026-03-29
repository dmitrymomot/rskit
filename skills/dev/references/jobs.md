# Background Jobs & Cron Scheduling

## Modules

- `modo::job` -- durable SQLite-backed job queue (feature-gated: `#[cfg(feature = "job")]`, depends on `db`)
- `modo::cron` -- in-process cron scheduler (always available, no feature gate)

Both modules are re-exported at the crate root as `pub mod job` and `pub mod cron`.

---

## Job System

### Database Table

Jobs are stored in the `jobs` table. The framework does **not** ship migrations -- the end-application owns its schema. You must create the table yourself with at minimum these columns: `id`, `name`, `queue`, `payload`, `payload_hash`, `status`, `attempt`, `run_at`, `started_at`, `completed_at`, `failed_at`, `error_message`, `created_at`, `updated_at`.

### Defining a Job Handler

A job handler is a plain `async fn` returning `modo::Result<()>`. Arguments must implement `FromJobContext`. Up to 12 arguments are supported.

Built-in extractors:

- `Payload<T>` -- deserializes the JSON payload into `T` (requires `T: DeserializeOwned`)
- `Service<T>` -- retrieves a service from the registry snapshot
- `Meta` -- job metadata (id, name, queue, attempt, max_attempts, deadline)

```rust
use modo::job::{Payload, Meta};
use modo::Service;
use serde::Deserialize;

#[derive(Deserialize)]
struct SendEmail { to: String, subject: String }

async fn send_email_job(
    payload: Payload<SendEmail>,
    meta: Meta,
    mailer: Service<MyMailer>,
) -> modo::Result<()> {
    tracing::info!(job_id = %meta.id, attempt = meta.attempt, "sending email");
    mailer.send(&payload.to, &payload.subject).await
}
```

`Payload<T>` implements `Deref<Target = T>`, so fields are accessible directly.

### Enqueuing Jobs

`Enqueuer` writes rows into `jobs`. Construct with a `Database` handle:

```rust
use modo::job::{Enqueuer, EnqueueOptions};

let enqueuer = Enqueuer::new(db);  // accepts Database (cloneable Arc<Connection>)

// Immediate execution on the "default" queue
let job_id = enqueuer.enqueue("send_email", &payload).await?;

// Delayed execution
let job_id = enqueuer.enqueue_at("send_email", &payload, run_at).await?;

// Full control (custom queue + schedule)
let job_id = enqueuer.enqueue_with("send_email", &payload, EnqueueOptions {
    queue: "emails".to_string(),
    run_at: Some(run_at),
}).await?;
```

#### Idempotent Enqueue

`enqueue_unique` / `enqueue_unique_with` deduplicate on a SHA-256 hash of `name + "\0" + payload_json`. If a pending or running job with the same hash exists, the existing job's ID is returned instead of inserting a duplicate.

```rust
use modo::job::EnqueueResult;

match enqueuer.enqueue_unique("send_email", &payload).await? {
    EnqueueResult::Created(id) => { /* new job */ }
    EnqueueResult::Duplicate(id) => { /* already queued */ }
}
```

Uniqueness relies on a partial unique index on `payload_hash` where `status IN ('pending', 'running')`. Since SQLite does not support `ON CONFLICT` with partial unique indexes, the code catches `is_unique_violation()` and falls back to a `SELECT`.

#### Cancelling Jobs

```rust
let cancelled: bool = enqueuer.cancel("job-id").await?;
```

Only cancels jobs still in `pending` status. Returns `false` if the job was not found or already past pending.

### Worker Configuration

`JobConfig` (`#[non_exhaustive]`) deserializes from YAML under the `job` key. All fields have defaults. Because the struct is `#[non_exhaustive]`, construct via `JobConfig { field: val, ..Default::default() }`:

| Field                        | Default                              | Description                                                                                     |
| ---------------------------- | ------------------------------------ | ----------------------------------------------------------------------------------------------- |
| `poll_interval_secs`         | `1`                                  | How often the worker polls for new jobs                                                         |
| `stale_threshold_secs`       | `600` (10 min)                       | Jobs stuck in `running` beyond this are reaped                                                  |
| `stale_reaper_interval_secs` | `60` (1 min)                         | How often the stale reaper runs                                                                 |
| `drain_timeout_secs`         | `30`                                 | Max wait for in-flight jobs during shutdown                                                     |
| `queues`                     | one `"default"` queue, concurrency 4 | List of `QueueConfig` entries                                                                   |
| `cleanup`                    | enabled, 1h interval, 72h retention  | Optional `CleanupConfig`                                                                        |
| `database`                   | `None`                               | Optional separate `db::Config` for the job queue DB; isolates job-queue writes from app queries |

#### Queue Config (`#[non_exhaustive]`)

`QueueConfig` is `#[non_exhaustive]` -- construct via `QueueConfig { name: ..., concurrency: ..., ..Default::default() }` or rely on YAML deserialization.

```yaml
job:
    queues:
        - name: default
          concurrency: 4
        - name: emails
          concurrency: 2
        - name: critical
          concurrency: 8
```

Each queue gets its own `Semaphore` with the specified concurrency limit. **Priority is handled by separate queues with different concurrency**, not by a numeric priority field.

#### Cleanup Config (`#[non_exhaustive]`)

`CleanupConfig` is `#[non_exhaustive]` -- construct via `CleanupConfig { ..Default::default() }` or YAML deserialization.

```yaml
job:
    cleanup:
        interval_secs: 3600 # run cleanup every hour
        retention_secs: 259200 # delete terminal jobs older than 72h
```

Terminal statuses: `completed`, `dead`, `cancelled`. Retention cutoff uses the `updated_at` column. Set `cleanup: ~` (null) to disable.

### Building and Starting a Worker

```rust
use modo::job::{Worker, JobOptions};

let worker = Worker::builder(&job_config, &registry)
    .register("send_email", send_email_job)
    .register_with("process_payment", process_payment_job, JobOptions {
        max_attempts: 5,
        timeout_secs: 60,
    })
    .start()
    .await;
```

`Worker::builder` panics if `Database` is not in the registry.

`Worker` implements `Task` for integration with the `run!` macro:

```rust
modo::run!(server, worker, scheduler);
```

### Per-Handler Options (`JobOptions`)

| Field          | Default       | Description                               |
| -------------- | ------------- | ----------------------------------------- |
| `max_attempts` | `3`           | Attempts before the job is marked `dead`  |
| `timeout_secs` | `300` (5 min) | Per-execution timeout; exceeded = failure |

### Retries and Exponential Backoff

On failure (handler error or timeout), if `attempt < max_attempts`, the job is reset to `pending` with a delayed `run_at`:

```
delay_secs = min(5 * 2^(attempt - 1), 3600)
```

Backoff progression: 5s, 10s, 20s, 40s, 80s, ... capped at 1 hour.

When `attempt >= max_attempts`, the job moves to `dead` status.

### Job Statuses

| Status      | Meaning                          |
| ----------- | -------------------------------- |
| `pending`   | Waiting to be picked up          |
| `running`   | Currently executing              |
| `completed` | Finished successfully            |
| `dead`      | Exhausted all retries            |
| `cancelled` | Cancelled via `Enqueuer::cancel` |

`Status` methods: `as_str()` returns the lowercase string (`"pending"`, `"running"`, etc.), `from_str(s)` parses back (returns `Option<Status>`), `is_terminal()` returns `true` for `completed`, `dead`, and `cancelled`. `Status` also implements `Display` (delegates to `as_str()`).

### Background Loops

`Worker::start()` spawns three tasks:

1. **Poll loop** -- claims pending jobs, dispatches to handlers with timeouts
2. **Stale reaper** -- resets `running` jobs whose `started_at` exceeds `stale_threshold_secs` back to `pending`
3. **Cleanup loop** (optional) -- deletes terminal jobs older than `retention_secs`

---

## Cron Scheduling

### Defining a Cron Handler

Same pattern as job handlers: plain `async fn` returning `modo::Result<()>`, up to 12 arguments.

Built-in extractors (via `FromCronContext`):

- `Service<T>` -- service from the registry snapshot
- `cron::Meta` -- metadata: `name` (handler type name), `deadline`, `tick` (scheduled `DateTime<Utc>`)

```rust
use modo::cron::Meta;
use modo::Service;

async fn cleanup_expired(
    meta: Meta,
    db: Service<MyDbService>,
) -> modo::Result<()> {
    tracing::info!(tick = %meta.tick, "running cleanup");
    db.delete_expired().await
}
```

Note: cron handlers do **not** have `Payload<T>` -- there is no payload to deserialize.

### Schedule Formats

Three formats are accepted:

**Standard cron expressions** -- 5-field or 6-field (with leading seconds):

```
*/5 * * * *        # every 5 minutes (5-field)
0 30 9 * * *       # daily at 09:30:00 (6-field with seconds)
```

**Named aliases**:

- `@yearly` / `@annually`
- `@monthly`
- `@weekly`
- `@daily` / `@midnight`
- `@hourly`

**Interval syntax** -- `@every <duration>` with `h`, `m`, `s` units:

```
@every 5m
@every 1h30m
@every 30s
```

Invalid expressions return an error at scheduler build time (the `job()` and `job_with()` methods return `Result<Self>`).

### Building and Starting a Scheduler

```rust
use modo::cron::{Scheduler, CronOptions};

let scheduler = Scheduler::builder(&registry)
    .job("@daily", cleanup_expired)?
    .job("*/5 * * * *", heartbeat)?
    .job_with("@every 30s", intensive_task, CronOptions {
        timeout_secs: 25,
        ..Default::default()
    })?
    .start()
    .await;
```

`Scheduler` implements `Task` for `run!` macro integration. Shutdown waits up to 30 seconds for in-flight executions.

### Per-Job Options (`CronOptions`, `#[non_exhaustive]`)

`CronOptions` is `#[non_exhaustive]` -- construct via `CronOptions { timeout_secs: 25, ..Default::default() }`.

| Field          | Default       | Description                 |
| -------------- | ------------- | --------------------------- |
| `timeout_secs` | `300` (5 min) | Max execution time per tick |

### Overlap Protection

If a previous execution is still running when the next tick fires, the tick is **skipped** with a warning log. There is no queue -- missed ticks are lost.

### Job Name

The cron `Meta.name` field is set to the fully qualified Rust type name of the handler function (`std::any::type_name::<H>()`), not a user-provided string.

---

## Gotchas

- **No embedded migrations**: The `job` module is DB-backed and `cron` is in-memory. The `jobs` table must be created by the end-application's migration. The framework does not ship DDL.

- **999 bind params limit (SQLite)**: The worker poll loop builds a dynamic `IN (?, ?, ...)` clause for all registered handler names. SQLite has a 999 bind parameter limit per statement, so a single worker can support a maximum of roughly 900 registered handlers.

- **croner 6-field support**: The framework uses `croner::parser::CronParser::builder().seconds(croner::parser::Seconds::Optional).build()` so both 5-field and 6-field (seconds-prefixed) expressions work. If you use `croner` directly elsewhere, remember to enable optional seconds for 6-field support.

- **`Database` required**: `Worker::builder()` panics if `Database` is not registered in the `Registry`.

- **Registry snapshot is frozen**: Both `Worker` and `Scheduler` capture a `RegistrySnapshot` at build time. Services added to the `Registry` after building are not visible to handlers.

- **Cron overlap skips ticks**: If a cron handler runs longer than the interval between ticks, subsequent ticks are skipped (not queued). Use a shorter interval or increase `timeout_secs` accordingly.

- **Exponential backoff formula**: `min(5 * 2^(attempt-1), 3600)` seconds. Attempt is 1-based. The cap is 1 hour.

- **Idempotent enqueue uses SHA-256**: Uniqueness key is `sha256(name + "\0" + payload_json)`. Different JSON serialization order of the same logical payload produces different hashes.

- **Named aliases expand to 6-field cron**: Aliases like `@daily` internally expand to `"0 0 0 * * *"` (6-field with seconds), not 5-field.

### Additional public types

These types are exported from `modo::job` and `modo::cron` but are rarely used directly:

- `JobContext`, `FromJobContext` -- context type and extractor trait for custom job extractors
- `JobHandler` -- handler trait (auto-implemented for matching `async fn`)
- `WorkerBuilder` (`#[must_use]`) -- the builder type returned by `Worker::builder()`
- `CronContext`, `FromCronContext` -- context type and extractor trait for custom cron extractors
- `CronHandler` -- handler trait (auto-implemented for matching `async fn`)
- `SchedulerBuilder` (`#[must_use]`) -- the builder type returned by `Scheduler::builder()`
