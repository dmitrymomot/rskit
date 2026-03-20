# modo v2 — Job Queue & Cron Scheduler Design Specification

## Overview

Two modules: `job` (DB-backed background job queue) and `cron` (in-memory recurring task scheduler). Both use extractor-based handlers matching the axum pattern. Always-on — no feature flags.

**New dependency:** `tokio-util` (with `rt` feature) for `CancellationToken` — used by both Worker and Scheduler for clean shutdown signaling.

**Cron parsing:** Use the `croner` crate for standard cron expression parsing. `@every <duration>` aliases are parsed in-house (simple duration string → `std::time::Duration`).

## Design Decisions

1. **Extractor-based handlers** — `FromJobContext` / `FromCronContext` traits, axum-style blanket impls over function signatures. `Service<T>` shared across HTTP, job, and cron contexts via three trait impls.
2. **Priority-weighted fetch** — single poller iterates queues in config order, claims `LIMIT available_slots` per queue. Per-queue concurrency via tokio semaphores.
3. **FIFO within each queue** — no priority field. Jobs ordered by `run_at ASC`. Separate queues are the priority mechanism.
4. **Enqueuer owns routing** — `queue` and `run_at` belong to the enqueue call. Handler owns execution policy — `max_attempts` and `timeout_secs`.
5. **Timeout via `tokio::time::timeout`** — handler deadline exposed in `Meta` extractor. Handler doesn't need to be timeout-aware; it gets cancelled.
6. **Panic detection via `JoinHandle`** — panics follow normal failure/retry path (not straight to dead).
7. **Dedicated stale reaper** — separate loop from polling (60s default). Recovers jobs stuck `running` after worker crash.
8. **Cleanup opt-out** — runs by default, 1h interval, 72h retention. Disable with `cleanup: null`.
9. **Cron overlap: skip-if-running** — stays on schedule. If previous run is still going when next tick fires, skip that tick.
10. **Separate `Meta` types** — `job::Meta` (id, name, queue, attempt, max_attempts, deadline) and `cron::Meta` (name, deadline, tick). No shared type with `Option` fields.
11. **Cron job names auto-derived** — from `std::any::type_name`. No configuration surface for custom names.
12. **All SQL timestamps computed in Rust** — bound as RFC 3339 string parameters. No `now()` or timestamp arithmetic in SQL. Matches session module pattern.
13. **Registry snapshot** — `Worker::builder()` and `Scheduler::builder()` clone the registry's internal `HashMap` into an `Arc` at construction time. The `Arc<RegistrySnapshot>` is shared across all spawned job/cron contexts. The original `Registry` can still be consumed by `into_state()` afterward.

## Database Schema

```sql
CREATE TABLE modo_jobs (
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
);

CREATE INDEX idx_modo_jobs_claimable
    ON modo_jobs (queue, status, run_at ASC)
    WHERE status = 'pending';

CREATE INDEX idx_modo_jobs_stale
    ON modo_jobs (status, started_at)
    WHERE status = 'running';

CREATE INDEX idx_modo_jobs_cleanup
    ON modo_jobs (status, updated_at);

CREATE INDEX idx_modo_jobs_unique
    ON modo_jobs (payload_hash, status)
    WHERE payload_hash IS NOT NULL AND status IN ('pending', 'running');
```

- `id` — ULID via `id::ulid()` (26 chars, time-sortable)
- `payload_hash` — nullable, SHA-256 of `name + payload`, only populated by `enqueue_unique()`
- `status` — text enum: `pending`, `running`, `completed`, `dead`, `cancelled`
- `attempt` — incremented by worker on each claim; checked against handler's `max_attempts`
- All timestamps as RFC 3339 strings (matching session module pattern)
- Partial indexes on hot query paths: claim, stale reap, cleanup, uniqueness check
- Cleanup index uses `updated_at` (not `completed_at`) — matches the cleanup query

**Note on `max_attempts`:** This value lives in the handler's `JobOptions` config, not in the DB. If a handler's `max_attempts` changes between deploys, the new value applies to all in-flight retries. This is intentional — the operator controls retry policy, not the enqueuer.

## Job Lifecycle

```
enqueue → Pending
            ↓ (atomic claim: UPDATE ... RETURNING, attempt incremented)
          Running
            ├── success → Completed
            ├── failure (attempt < max_attempts) → Pending (exponential backoff via run_at)
            ├── failure (attempt >= max_attempts) → Dead
            ├── timeout → failure path
            └── panic (JoinHandle) → failure path
cancel → Cancelled (from Pending only)
stale reaper → Running back to Pending (worker crash recovery, attempt unchanged)
cleanup → deletes terminal states after retention period
```

Backoff formula: `delay_secs = min(5 * 2^(attempt - 1), 3600)`. Applied by setting `run_at = Utc::now() + delay` (computed in Rust, bound as RFC 3339).

## Core Types

### Job Module

```rust
pub enum Status {
    Pending,
    Running,
    Completed,
    Dead,
    Cancelled,
}

pub struct Meta {
    pub id: String,
    pub name: String,
    pub queue: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub deadline: Option<tokio::time::Instant>,
}

pub enum EnqueueResult {
    Created(String),
    Duplicate(String),
}

pub struct EnqueueOptions {
    pub queue: String,                  // default: "default"
    pub run_at: Option<DateTime<Utc>>,  // default: now
}

pub struct JobOptions {
    pub max_attempts: u32,    // default: 3
    pub timeout_secs: u64,    // default: 300
}
```

**Raw payload access:** Handlers that need untyped JSON access use `Payload<serde_json::Value>`. There is no separate raw payload field on `Meta`.

### Cron Module

```rust
pub struct Meta {
    pub name: String,                         // auto-derived from std::any::type_name
    pub deadline: Option<tokio::time::Instant>,
    pub tick: DateTime<Utc>,                  // scheduled fire time
}

pub struct CronOptions {
    pub timeout_secs: u64,  // default: 300
}
```

## Enqueuer

```rust
pub struct Enqueuer {
    writer: InnerPool,
}

impl Enqueuer {
    pub fn new(writer: &impl Writer) -> Self;

    /// Enqueue for immediate execution on the "default" queue.
    pub async fn enqueue<T: Serialize>(
        &self, name: &str, payload: &T,
    ) -> Result<String>;

    /// Enqueue for execution at a specific time on the "default" queue.
    pub async fn enqueue_at<T: Serialize>(
        &self, name: &str, payload: &T, run_at: DateTime<Utc>,
    ) -> Result<String>;

    /// Enqueue with full options (queue, run_at).
    pub async fn enqueue_with<T: Serialize>(
        &self, name: &str, payload: &T, options: EnqueueOptions,
    ) -> Result<String>;

    /// Deduplicated enqueue. Returns Duplicate(id) if a pending/running job
    /// with the same name+payload already exists; Created(id) otherwise.
    /// No TTL — deduplication is active as long as a matching job is
    /// pending or running. Once the job completes/dies/is cancelled,
    /// the same payload can be enqueued again.
    pub async fn enqueue_unique<T: Serialize>(
        &self, name: &str, payload: &T,
    ) -> Result<EnqueueResult>;

    /// Cancel a pending job. Returns true if cancelled, false if not found
    /// or already running.
    pub async fn cancel(&self, id: &str) -> Result<bool>;
}
```

- `Enqueuer` only needs a write pool — insert-only
- `enqueue()` and `enqueue_at()` are convenience methods over `enqueue_with()`
- Added to registry as `Service<Enqueuer>` in HTTP handlers

### Deduplication Query

`enqueue_unique()` computes `SHA-256(name + serde_json::to_string(payload))` and checks:

```sql
SELECT id FROM modo_jobs
WHERE payload_hash = ?
  AND status IN ('pending', 'running')
LIMIT 1
```

If a row is found, return `EnqueueResult::Duplicate(id)`. Otherwise, insert the job with `payload_hash` set, and return `EnqueueResult::Created(id)`. The `payload_hash` includes the job name, so a separate `name` filter is redundant.

## Worker

```rust
pub struct Worker { /* internal */ }

impl Worker {
    pub fn builder(config: &JobConfig, registry: &Registry) -> WorkerBuilder;
}

pub struct WorkerBuilder { /* internal */ }

impl WorkerBuilder {
    /// Register handler with defaults (max_attempts=3, timeout=300s).
    pub fn register<H, Args>(self, name: &str, handler: H) -> Self
    where
        H: JobHandler<Args>;

    /// Register handler with custom execution policy.
    pub fn register_with<H, Args>(
        self, name: &str, handler: H, options: JobOptions,
    ) -> Self
    where
        H: JobHandler<Args>;

    /// Start the worker. Spawns poll loop, stale reaper, and cleanup loop.
    pub async fn start(self) -> Worker;
}

impl Task for Worker {
    async fn shutdown(self) -> Result<()>;
    // 1. Cancel CancellationToken (stops poll loop, reaper, cleanup)
    // 2. Wait up to drain_timeout_secs for in-flight jobs
    // 3. Force-cancel remaining via JoinHandle abort
}
```

### Polling Loop

All timestamp values are computed in Rust as `Utc::now().to_rfc3339()` and bound as query parameters. Handler name list is included as bind parameters — practical limit is several hundred handlers per the SQLite default of 999 bind parameters, well beyond typical usage.

```
loop:
    select! {
        _ = cancellation_token.cancelled() => break,
        _ = sleep(poll_interval) => {
            let now = Utc::now().to_rfc3339();
            for each queue in config order:
                slots = semaphore.available_permits()
                if slots == 0: continue

                claimed = UPDATE modo_jobs
                    SET status = 'running',
                        attempt = attempt + 1,
                        started_at = ?,  -- now (bound from Rust)
                        updated_at = ?   -- now (bound from Rust)
                    WHERE id IN (
                        SELECT id FROM modo_jobs
                        WHERE status = 'pending'
                          AND run_at <= ?  -- now (bound from Rust)
                          AND queue = ?
                          AND name IN (?, ?, ...)  -- registered handler names
                        ORDER BY run_at ASC
                        LIMIT ?  -- available semaphore permits
                    )
                    RETURNING *

                for each job in claimed:
                    acquire semaphore permit
                    tokio::spawn:
                        build JobContext (registry snapshot + payload + meta + deadline)
                        match tokio::time::timeout(duration, handler.call(ctx)):
                            Ok(Ok(())) → mark completed
                            Ok(Err(e)) → attempt < max ? reschedule with backoff : mark dead
                            Err(elapsed) → same as Err (timeout)
                        JoinHandle panic → same as Err
                        release semaphore permit
        }
    }
```

### Stale Reaper

Dedicated loop running every `stale_reaper_interval_secs` (default 60s). Uses `CancellationToken` for shutdown. All timestamps computed in Rust:

```rust
let threshold = (Utc::now() - chrono::Duration::seconds(stale_threshold_secs)).to_rfc3339();
let now = Utc::now().to_rfc3339();

sqlx::query(
    "UPDATE modo_jobs SET status = 'pending', started_at = NULL, updated_at = ? \
     WHERE status = 'running' AND started_at < ?"
)
.bind(&now)
.bind(&threshold)
.execute(&writer).await?;
```

Recovers jobs stuck `running` after worker process crash. Attempt count is unchanged — it was already incremented on claim.

### Cleanup Loop

Dedicated loop running every `cleanup.interval_secs` (default 3600s). Uses `CancellationToken` for shutdown. All timestamps computed in Rust:

```rust
let threshold = (Utc::now() - chrono::Duration::seconds(retention_secs)).to_rfc3339();

sqlx::query(
    "DELETE FROM modo_jobs WHERE status IN ('completed', 'dead', 'cancelled') AND updated_at < ?"
)
.bind(&threshold)
.execute(&writer).await?;
```

Default retention: 72h (259200s). Cleanup always deletes terminal states (`completed`, `dead`, `cancelled`) — this is not configurable. Opt-out of the entire cleanup loop by setting `cleanup: null` in config.

## Handler Traits

### JobHandler

```rust
pub trait JobHandler<Args>: Clone + Send + 'static {
    fn call(self, ctx: JobContext) -> impl Future<Output = Result<()>> + Send;
}

pub trait FromJobContext: Sized {
    fn from_job_context(ctx: &JobContext) -> Result<Self>;
}

// Implementations (in job/context.rs):
// - Payload<T>: deserializes ctx.payload JSON string into T
// - Service<T>: reads from ctx.registry (FromJobContext impl on extractor::Service<T>)
// - job::Meta: clones ctx.meta
```

Blanket impls via macro for 0..12 extractor tuples:

```rust
// Example for 2 args:
impl<F, Fut, T1, T2> JobHandler<(T1, T2)> for F
where
    F: FnOnce(T1, T2) -> Fut + Clone + Send + 'static,
    Fut: Future<Output = Result<()>> + Send,
    T1: FromJobContext,
    T2: FromJobContext,
{
    fn call(self, ctx: JobContext) -> impl Future<Output = Result<()>> + Send {
        async move {
            let t1 = T1::from_job_context(&ctx)?;
            let t2 = T2::from_job_context(&ctx)?;
            (self)(t1, t2).await
        }
    }
}
```

### CronHandler

```rust
pub trait CronHandler<Args>: Clone + Send + 'static {
    fn call(self, ctx: CronContext) -> impl Future<Output = Result<()>> + Send;
}

pub trait FromCronContext: Sized {
    fn from_cron_context(ctx: &CronContext) -> Result<Self>;
}

// Implementations (in cron/context.rs):
// - Service<T>: reads from ctx.registry (FromCronContext impl on extractor::Service<T>)
// - cron::Meta: clones ctx.meta
// (No Payload — cron jobs have no input data)
```

Same macro pattern for 0..12 extractor tuples.

### Contexts (internal)

```rust
pub(crate) struct JobContext {
    registry: Arc<RegistrySnapshot>,
    payload: String,    // JSON text from DB, deserialized by Payload<T> extractor
    meta: job::Meta,
}

pub(crate) struct CronContext {
    registry: Arc<RegistrySnapshot>,
    meta: cron::Meta,
}
```

`RegistrySnapshot` is a `HashMap<TypeId, Arc<dyn Any + Send + Sync>>` cloned from the `Registry` at builder construction time. It supports the same `get::<T>() -> Option<Arc<T>>` lookup that `Service<T>` needs.

### Extractor Impl Locations

- `FromJobContext for Service<T>` — implemented in `job/context.rs`
- `FromJobContext for Payload<T>` — implemented in `job/context.rs`
- `FromJobContext for job::Meta` — implemented in `job/context.rs`
- `FromCronContext for Service<T>` — implemented in `cron/context.rs`
- `FromCronContext for cron::Meta` — implemented in `cron/context.rs`

All impls are crate-local (single crate), so no orphan rule concerns.

## Cron Scheduler

```rust
pub struct Scheduler { /* internal */ }

impl Scheduler {
    pub fn builder(registry: &Registry) -> SchedulerBuilder;
}

pub struct SchedulerBuilder { /* internal */ }

impl SchedulerBuilder {
    pub fn job<H, Args>(self, schedule: &str, handler: H) -> Self
    where
        H: CronHandler<Args>;

    pub fn job_with<H, Args>(
        self, schedule: &str, handler: H, options: CronOptions,
    ) -> Self
    where
        H: CronHandler<Args>;

    /// Start the scheduler. Panics if any schedule string is invalid.
    pub async fn start(self) -> Scheduler;
}

impl Task for Scheduler {
    async fn shutdown(self) -> Result<()>;
    // 1. Cancel CancellationToken (stops all job loops)
    // 2. Wait for in-flight handlers to finish (with drain timeout)
}
```

### Schedule Formats

Standard cron expressions parsed by the `croner` crate. `@every` and named aliases parsed in-house.

| Format                  | Example                              |
| ----------------------- | ------------------------------------ |
| `@yearly` / `@annually` | Midnight Jan 1                      |
| `@monthly`              | Midnight 1st of month               |
| `@weekly`               | Midnight Sunday                     |
| `@daily` / `@midnight`  | Midnight                            |
| `@hourly`               | Top of every hour                   |
| `@every <duration>`     | `1h`, `30m`, `15s`, `1h30m`         |
| Standard cron           | `0 0 9 * * MON-FRI` (6-field)      |

Validated at `start()` — invalid schedule panics. Fail fast.

### Per-Job Execution Loop

```
1. Parse schedule, compute next_tick
2. select! {
       _ = cancellation_token.cancelled() => return,
       _ = tokio::time::sleep_until(next_tick) => { ... }
   }
3. Check skip flag (AtomicBool) — if previous run still going, skip, compute next_tick, goto 2
4. Set skip flag
5. tokio::time::timeout(duration, handler.call(ctx))
6. Log result (success or error) — no retries, no persistence
7. Clear skip flag
8. Compute next_tick, goto 2
```

- Purely in-memory — no DB, no state persistence
- Each cron job is its own `tokio::spawn` loop
- Errors are logged and swallowed — next tick runs fresh
- `CancellationToken` from `tokio_util` for clean shutdown signaling
- Skip flag is `AtomicBool` — lock-free overlap detection

## Configuration

```yaml
job:
    poll_interval_secs: 1
    stale_threshold_secs: 600
    stale_reaper_interval_secs: 60
    drain_timeout_secs: 30
    queues:
        - name: default
          concurrency: 4
        - name: email
          concurrency: 2
    cleanup:
        interval_secs: 3600
        retention_secs: 259200
```

Cleanup always targets terminal states (`completed`, `dead`, `cancelled`). No `statuses` config field — this is not user-configurable; terminal states are an implementation invariant.

Cron has no config struct — schedule strings and options are set in code.

## File Layout

```
src/
  job/
    mod.rs          -- mod imports, re-exports (JobConfig, Enqueuer, EnqueueOptions,
                       EnqueueResult, Worker, WorkerBuilder, JobOptions, JobHandler,
                       FromJobContext, Payload, Meta, Status)
    config.rs       -- JobConfig, QueueConfig, CleanupConfig
    enqueuer.rs     -- Enqueuer, EnqueueOptions, EnqueueResult
    worker.rs       -- Worker, WorkerBuilder, JobOptions
    handler.rs      -- JobHandler trait, blanket impls (macro)
    context.rs      -- JobContext (pub(crate)), FromJobContext trait,
                       FromJobContext impls for Service<T>, Payload<T>, Meta
    payload.rs      -- Payload<T> extractor type
    meta.rs         -- job::Meta, job::Status
    reaper.rs       -- stale reaper loop
    cleanup.rs      -- cleanup loop
  cron/
    mod.rs          -- mod imports, re-exports (Scheduler, SchedulerBuilder,
                       CronOptions, CronHandler, FromCronContext, Meta)
    scheduler.rs    -- Scheduler, SchedulerBuilder, CronOptions
    handler.rs      -- CronHandler trait, blanket impls (macro)
    context.rs      -- CronContext (pub(crate)), FromCronContext trait,
                       FromCronContext impls for Service<T>, Meta
    meta.rs         -- cron::Meta
    schedule.rs     -- schedule parsing (@every, @daily, standard cron via croner)
```

## Bootstrap Example

```rust
use modo::{config, db, server, service, job, cron};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::load::<AppConfig>("config/")?;
    let _guard = modo::tracing::init(&config.modo.tracing)?;

    let (reader, writer) = db::connect_rw(&config.modo.database).await?;
    db::migrate("./migrations", &writer).await?;

    let mut registry = service::Registry::new();
    registry.add(reader.clone());
    registry.add(writer.clone());
    registry.add(job::Enqueuer::new(&writer));

    let worker = job::Worker::builder(&config.modo.job, &registry)
        .register("send_confirmation", send_confirmation)
        .register_with("heavy_task", heavy_task, job::JobOptions {
            max_attempts: 5,
            timeout_secs: 600,
        })
        .start().await;

    let scheduler = cron::Scheduler::builder(&registry)
        .job("@every 15m", cleanup_sessions)
        .job_with("@every 30s", health_check, cron::CronOptions {
            timeout_secs: 10,
        })
        .start().await;

    let router = axum::Router::new()
        .nest("/api/orders", order::routes())
        .with_state(registry.into_state());

    let server = server::http(router, &config.modo.server).await;

    modo::runtime::run!(server, worker, scheduler, db::managed(writer), db::managed(reader)).await
}

// Job handler — uses extractors like HTTP handlers
async fn send_confirmation(
    payload: job::Payload<OrderPayload>,
    meta: job::Meta,
    Service(mailer): Service<Mailer>,
) -> modo::Result<()> {
    // meta.deadline available for sub-timeouts
    mailer.send(&payload.email, "confirmation").await
}

// Cron handler
async fn cleanup_sessions(
    Service(db): Service<db::WritePool>,
) -> modo::Result<()> {
    sqlx::query("DELETE FROM modo_sessions WHERE expires_at < ?")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&*db).await?;
    Ok(())
}

// Enqueue from HTTP handler
async fn create_order(
    Service(jobs): Service<job::Enqueuer>,
) -> modo::Result<axum::Json<Order>> {
    let order = Order { /* ... */ };
    jobs.enqueue("send_confirmation", &OrderPayload { email: order.email.clone() }).await?;
    Ok(axum::Json(order))
}
```
