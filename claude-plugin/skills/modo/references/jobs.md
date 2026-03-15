# Background Jobs Reference

The `modo-jobs` crate provides persistent, database-backed background job processing.
Jobs are defined with the `#[job]` attribute macro, enqueued via the `JobQueue` extractor
in HTTP handlers, and executed by a runner that polls the database on configurable intervals.
Cron jobs run in-memory on a schedule but are not persisted to the database.

---

## Documentation

- modo-jobs crate: https://docs.rs/modo-jobs
- modo-jobs-macros crate: https://docs.rs/modo-jobs-macros

---

## Job Definition

Use the `#[job]` attribute from `modo_jobs` to annotate an `async` function.

The macro generates:
- A unit struct `<FnName>Job` (PascalCase of the function name) implementing `JobHandler`.
- A `JOB_NAME` constant on the struct set to the function name string.
- `enqueue` and `enqueue_at` associated functions on the struct (omitted for cron jobs).
- An `inventory` registration entry — no explicit startup call is needed.

### Attribute Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `queue` | string | `"default"` | Target queue name. Must match a configured queue in `JobsConfig`. |
| `priority` | integer | `0` | Higher values run first within the same queue. |
| `max_attempts` | integer | `3` | Retry limit before the job is marked `dead`. |
| `timeout` | string (`"Xs"`, `"Xm"`, `"Xh"`) | `"5m"` | Per-execution timeout. |
| `cron` | string (cron expression) | — | Recurring in-memory schedule. Mutually exclusive with `queue`, `priority`, and `max_attempts`. |

### Function Signature Rules

- The function must be `async`.
- Return type must be `Result<(), modo::Error>` or any alias thereof (e.g. `HandlerResult<()>`).
- At most one plain parameter is treated as the **payload** (deserialized from JSON).
- Use `Service<T>` to inject a registered service into the job handler.
- Use `Db` to inject the database pool.
- Multiple payload parameters are a compile error.

### Payload-based job

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
// Generates: SendWelcomeJob with SendWelcomeJob::enqueue and SendWelcomeJob::enqueue_at
```

### Job with service injection

```rust
use modo_jobs::job;
use modo::HandlerResult;
use modo::Service;

#[job(queue = "mailer", timeout = "1m")]
async fn send_report(
    payload: ReportPayload,
    Service(mailer): Service<MyMailer>,
) -> HandlerResult<()> {
    mailer.send(payload.to, payload.subject).await?;
    Ok(())
}
```

### Job with database access

```rust
use modo_jobs::job;
use modo::HandlerResult;
use modo_db::Db;

#[job(queue = "default")]
async fn sync_user(payload: SyncPayload, Db(db): Db) -> HandlerResult<()> {
    // use db (DbPool) directly
    Ok(())
}
```

### Cron job (no payload, no queue)

```rust
#[job(cron = "0 */1 * * * *", timeout = "30s")]
async fn heartbeat() -> HandlerResult<()> {
    tracing::info!("heartbeat tick");
    Ok(())
}
```

Cron expressions use the `cron` crate's six-field format: `second minute hour day month weekday`.
The `cron` attribute is mutually exclusive with `queue`, `priority`, and `max_attempts` — specifying
both is a compile error.

---

## Enqueuing Jobs

`JobQueue` is an Axum extractor that implements `FromRequestParts<AppState>`. It resolves from
the `JobsHandle` registered as a managed service. Add it as a parameter to any handler.

```rust
use modo_jobs::JobQueue;
use crate::jobs::SayHelloJob;
use crate::payloads::GreetingPayload;
use modo::{Json, JsonResult};
use serde_json::{Value, json};

#[modo::handler(POST, "/jobs/greet")]
async fn enqueue_greet(queue: JobQueue, input: Json<GreetingPayload>) -> JsonResult<Value> {
    let job_id = SayHelloJob::enqueue(&queue, &input).await?;
    Ok(Json(json!({ "job_id": job_id.to_string() })))
}
```

### Enqueue for future execution

Use `enqueue_at` to schedule a job to run no earlier than a specific UTC timestamp:

```rust
#[modo::handler(POST, "/jobs/remind")]
async fn enqueue_remind(queue: JobQueue, input: Json<ReminderPayload>) -> JsonResult<Value> {
    let run_at = chrono::Utc::now() + chrono::Duration::seconds(10);
    let job_id = RemindJob::enqueue_at(&queue, &input, run_at).await?;
    Ok(Json(json!({ "job_id": job_id.to_string(), "run_at": run_at.to_rfc3339() })))
}
```

### Cancel a pending job

```rust
queue.cancel(&job_id).await?;
```

Only jobs in the `Pending` state can be cancelled. Returns an error if the job is not found
or is not in the `Pending` state.

### JobId

`JobId` is a ULID-backed string identifier returned by `enqueue` and `enqueue_at`.

```rust
let job_id: JobId = SayHelloJob::enqueue(&queue, &payload).await?;
println!("{}", job_id); // prints ULID string
```

---

## Configuration

Add `JobsConfig` to your app config struct and deserialize it from YAML.

```rust
use modo_db::DatabaseConfig;
use modo_jobs::JobsConfig;
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub struct Config {
    #[serde(flatten)]
    pub core: modo::config::AppConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub jobs: JobsConfig,
}
```

### JobsConfig fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `poll_interval_secs` | `u64` | `1` | How often each queue polls for new jobs. |
| `stale_threshold_secs` | `u64` | `600` | Jobs locked longer than this are re-queued. |
| `drain_timeout_secs` | `u64` | `30` | Max time to wait for in-flight jobs at shutdown. |
| `queues` | `Vec<QueueConfig>` | `[{name: "default", concurrency: 4}]` | Per-queue configuration. |
| `cleanup` | `CleanupConfig` | see below | Automatic cleanup of finished jobs. |
| `max_payload_bytes` | `Option<usize>` | `None` (unlimited) | Optional cap on serialized payload size. |

### QueueConfig fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Queue name (must match `#[job(queue = "...")]`). |
| `concurrency` | `usize` | Maximum concurrent jobs on this queue. Must be > 0. |

### CleanupConfig fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `interval_secs` | `u64` | `3600` | How often the cleanup task runs. |
| `retention_secs` | `u64` | `86400` | Jobs older than this are eligible for deletion. |
| `statuses` | `Vec<JobState>` | `[Completed, Dead, Cancelled]` | Which states to clean up. |

Example YAML:

```yaml
jobs:
  poll_interval_secs: 1
  stale_threshold_secs: 600
  drain_timeout_secs: 30
  queues:
    - name: default
      concurrency: 4
    - name: email
      concurrency: 2
  cleanup:
    interval_secs: 3600
    retention_secs: 86400
    statuses: [completed, dead, cancelled]
```

---

## Starting the Runner

Call `modo_jobs::new()` to create a `JobsBuilder`, register services, and call `.run()`.
Register the resulting `JobsHandle` as a managed service so the framework calls graceful
shutdown on it.

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

`JobsBuilder::service<T>` registers any `Send + Sync + 'static` value into a `ServiceRegistry`
that is passed to every job handler via `JobContext`. Call `.service()` as many times as needed
before calling `.run()`.

`JobsBuilder::run()` starts per-queue poll loops, a stale reaper, a cleanup task, and the
cron scheduler — all as separate Tokio tasks. It returns an error if configuration validation
fails or if a registered job references a queue that is not in `config.queues`.

---

## Retry and Exponential Backoff

When a job handler returns an `Err(_)` or times out, the runner checks `attempts` against
`max_attempts`:

- If `attempts < max_attempts`: the job is rescheduled to `Pending` with exponential backoff.
- If `attempts >= max_attempts`: the job is marked `Dead` and will not be retried.

The backoff formula is:

```
backoff_secs = min(5 * 2^(attempt - 1), 3600)
```

| Attempt | Delay |
|---------|-------|
| 1st retry | 5 s |
| 2nd retry | 10 s |
| 3rd retry | 20 s |
| 4th retry | 40 s |
| ... | doubles each time |
| cap | 3600 s (1 hour) |

The `last_error` field on the `Job` entity stores the error message from the most recent
failed attempt.

Configure `max_attempts` on the job itself:

```rust
#[job(queue = "default", max_attempts = 5)]
async fn flaky_task(payload: TaskPayload) -> HandlerResult<()> {
    // ...
}
```

The default is 3. Set `max_attempts = 1` to disable retries entirely.

---

## Job State Machine

`JobState` tracks the lifecycle of every job in the `modo_jobs` table.

| State | Meaning |
|-------|---------|
| `Pending` | Waiting to be picked up by a worker. |
| `Running` | Currently executing on a worker. |
| `Completed` | Finished successfully. |
| `Dead` | Exhausted all retry attempts without success. |
| `Cancelled` | Cancelled before execution via `JobQueue::cancel`. |

States are stored as lowercase strings in the database (`"pending"`, `"running"`, etc.).
The `JobState` enum implements `Display`, `FromStr`, and `Serialize`/`Deserialize`.

---

## Cron Scheduling

Cron jobs are declared with `#[job(cron = "...")]`. They run entirely in-memory — no record
is written to the `modo_jobs` table for individual executions.

```rust
#[job(cron = "0 */5 * * * *", timeout = "1m")]
async fn cleanup_expired_sessions() -> HandlerResult<()> {
    tracing::info!("Cleaning up expired sessions");
    Ok(())
}
```

The cron expression uses six fields: `second minute hour day month weekday`.

**Execution semantics:**
- One Tokio task is spawned per cron job at startup.
- At most one instance of each cron job runs at a time.
- If the handler takes longer than the interval between ticks, the next tick is skipped rather
  than firing concurrently.
- Each execution receives a fresh `JobContext` with `attempt = 1` and `payload_json = "null"`.
- If a cron job fails five consecutive times, a warning is logged.
- Cron tasks respect the `CancellationToken` and stop cleanly on shutdown.

**Cron jobs support the same parameter injection as regular jobs** (`Service<T>`, `Db`, etc.).
The macro generates the same extraction code for all job types. The cron runner creates a fresh
`JobContext` with all registered services for each execution, so `Service<T>` and `Db` work
identically in cron job function signatures.

---

## Graceful Shutdown

`JobsHandle` implements `modo::GracefulShutdown` at the `Drain` shutdown phase. Registering
it via `app.managed_service(jobs)` is all that is required — the framework calls
`JobsHandle::shutdown()` automatically.

Shutdown sequence:
1. `CancellationToken` is cancelled, signalling all poll loops, the stale reaper, the cleanup
   task, and all cron tasks to stop accepting new work.
2. The runner waits up to `drain_timeout_secs` for all in-flight jobs to finish by acquiring
   all semaphore permits for each queue.
3. If the drain timeout expires before all jobs complete, a warning is logged and the process
   proceeds with shutdown regardless.

To get the cancellation token for custom use:

```rust
let token = jobs.cancel_token();
```

---

## Integration Patterns

### Accessing the database in a job

Register the `DbPool` as a service and use the `Db` extractor:

```rust
// In main:
let jobs = modo_jobs::new(&db, &config.jobs)
    .service(db.clone())
    .run()
    .await?;

// In job definition:
#[job(queue = "default")]
async fn process_record(payload: RecordPayload, Db(db): Db) -> HandlerResult<()> {
    let pool = db.clone(); // Arc<DbPool>
    // query using SeaORM via pool.connection()
    Ok(())
}
```

### Accessing a custom service (e.g. Mailer)

```rust
// In main:
let mailer = MyMailer::new(&config.mailer);
let jobs = modo_jobs::new(&db, &config.jobs)
    .service(db.clone())
    .service(mailer)
    .run()
    .await?;

// In job definition:
#[job(queue = "mailer")]
async fn send_notification(
    payload: NotificationPayload,
    Service(mailer): Service<MyMailer>,
) -> HandlerResult<()> {
    mailer.send(&payload.to, &payload.body).await?;
    Ok(())
}
```

### Enqueuing from outside HTTP handlers

`JobsHandle` derefs to `JobQueue`, so you can call enqueue methods directly on the handle:

```rust
let job_id = SayHelloJob::enqueue(&*jobs, &payload).await?;
```

Or pass the `JobsHandle` (or a cloned `JobQueue`) wherever needed.

### Email integration (modo-email)

The mailer is registered as a jobs service (`.service(email)` on the jobs builder), not on
the app. The app enqueues a `SendEmailPayload`; the job worker sends the email.

```rust
// Register mailer as a jobs service:
let jobs = modo_jobs::new(&db, &config.jobs)
    .service(db.clone())
    .service(mailer)  // mailer is a jobs service, NOT an app service
    .run()
    .await?;

// In the send-email job:
#[job(queue = "email")]
async fn send_email(
    payload: SendEmailPayload,
    Service(mailer): Service<Mailer>,
) -> HandlerResult<()> {
    mailer.send(&SendEmail::from(payload)).await?;
    Ok(())
}
```

---

## Gotchas

**Cron jobs are not persisted.** Individual cron executions never appear in the `modo_jobs`
table. If the process restarts between ticks, the cron schedule resets from the next upcoming
time rather than catching up on missed ticks.

**Queue names must be configured.** Every job whose `queue` parameter does not match a name in
`config.queues` causes `JobsBuilder::run()` to return an error at startup. Cron jobs bypass
this check.

**`inventory` registration in tests.** Jobs are registered via `inventory::submit!` at link
time. In test binaries, the linker may not include the library object file unless there is a
direct reference to it. Force inclusion with:

```rust
use crate::jobs::send_welcome as _;
```

This applies whenever `cargo test` fails to discover jobs that work in normal builds.

**Payload size limit.** If `max_payload_bytes` is set in `JobsConfig`, `enqueue` returns an
error when the serialized payload exceeds that limit. The default is `None` (unlimited).

**`max_attempts` is per-job, not per-queue.** Each `#[job]` declaration sets its own
`max_attempts`. There is no global override in `JobsConfig`.

**Stale job reaping.** Jobs locked longer than `stale_threshold_secs` (default 600 s) are
reset to `Pending` and their attempt counter is decremented. This handles crashed workers.
The reaper runs every 60 seconds.

**Cron timeout panics on invalid expressions.** A malformed cron expression causes a `panic!`
at startup rather than returning an error. Verify expressions before deploying.

**`JobQueue` requires `JobsHandle` to be registered.** If `JobsHandle` is not in the service
registry, the `JobQueue` extractor returns a 500 error with the message:
`"JobQueue not configured. Start the job runner and register JobsHandle as a service."`

---

## Key Types Quick Reference

| Type | Crate | Description |
|------|-------|-------------|
| `JobsConfig` | `modo_jobs::config` | Top-level configuration (queues, timeouts, cleanup). |
| `QueueConfig` | `modo_jobs::config` | Per-queue name and concurrency. |
| `CleanupConfig` | `modo_jobs::config` | Cleanup interval, retention, and target states. |
| `JobsBuilder` | `modo_jobs::runner` | Builder returned by `modo_jobs::new()`. |
| `JobsHandle` | `modo_jobs::runner` | Live handle returned by `JobsBuilder::run()`. Derefs to `JobQueue`. |
| `JobQueue` | `modo_jobs::queue` | Enqueue / cancel operations. Axum extractor. |
| `JobId` | `modo_jobs::types` | ULID-backed job identifier. |
| `JobState` | `modo_jobs::types` | `Pending`, `Running`, `Completed`, `Dead`, `Cancelled`. |
| `JobContext` | `modo_jobs::handler` | Runtime context passed to a job handler. |
| `JobHandler` | `modo_jobs::handler` | Trait implemented by the `#[job]` macro. |
| `JobHandlerDyn` | `modo_jobs::handler` | Object-safe bridge for `Box<dyn JobHandlerDyn>`. |
| `JobRegistration` | `modo_jobs::handler` | Inventory entry created by `#[job]`. |
