# modo::cron

Periodic cron job scheduling for the modo framework.

Handlers are plain `async fn` values — no macros, no derives. Services are
injected via the [`Registry`](../service/struct.Registry.html) snapshot
captured at build time. The scheduler integrates with the runtime's `run!`
macro through the `Task` trait for clean shutdown.

## Schedule Formats

Three formats are accepted wherever a schedule string is required:

| Format                                   | Examples                                                                        |
| ---------------------------------------- | ------------------------------------------------------------------------------- |
| Standard cron (5-field)                  | `"0 30 9 * * *"`, `"*/5 * * * *"`                                               |
| Standard cron (6-field, leading seconds) | `"0 0 30 9 * * *"`                                                              |
| Named alias                              | `@yearly`, `@annually`, `@monthly`, `@weekly`, `@daily`, `@midnight`, `@hourly` |
| Interval                                 | `@every 5m`, `@every 1h30m`, `@every 30s`                                       |

Invalid expressions or durations cause a panic at startup.

## Key Types

| Type                | Description                                              |
| ------------------- | -------------------------------------------------------- |
| `Scheduler`         | Running scheduler handle; implements `Task` for shutdown |
| `SchedulerBuilder`  | Builder returned by `Scheduler::builder()`               |
| `CronOptions`       | Per-job options (timeout)                                |
| `Meta`              | Job metadata injected into handler arguments             |
| `CronContext`       | Full execution context; not used directly by handlers    |
| `CronHandler<Args>` | Trait implemented automatically for matching `async fn`  |
| `FromCronContext`   | Trait for types extractable from `CronContext`           |

## Basic Usage

```rust
use modo::cron::{Scheduler, Meta};
use modo::extractor::Service;
use modo::service::Registry;
use modo::runtime::Task;
use modo::Result;
use std::sync::Arc;

struct EmailService;

async fn send_digest(svc: Service<Arc<EmailService>>) -> Result<()> {
    // use svc.0 to call methods on EmailService
    Ok(())
}

async fn heartbeat() -> Result<()> {
    tracing::info!("heartbeat");
    Ok(())
}

#[tokio::main]
async fn main() {
    let mut registry = Registry::new();
    registry.add(Arc::new(EmailService));

    let scheduler = Scheduler::builder(&registry)
        .job("@daily", send_digest)
        .job("@every 1m", heartbeat)
        .start()
        .await;

    // Integrate with the run! macro for graceful shutdown, or shut down manually:
    scheduler.shutdown().await.unwrap();
}
```

## Custom Timeout

```rust
use modo::cron::{Scheduler, CronOptions};
use modo::service::Registry;
use modo::Result;

async fn slow_job() -> Result<()> {
    Ok(())
}

#[tokio::main]
async fn main() {
    let registry = Registry::new();

    let scheduler = Scheduler::builder(&registry)
        .job_with(
            "@hourly",
            slow_job,
            CronOptions { timeout_secs: 600 },
        )
        .start()
        .await;
}
```

## Accessing Job Metadata

Declare `Meta` as a handler argument to receive the job name, scheduled tick
time, and execution deadline:

```rust
use modo::cron::Meta;
use modo::Result;

async fn logged_job(meta: Meta) -> Result<()> {
    tracing::info!(
        job = %meta.name,
        tick = %meta.tick,
        "running"
    );
    Ok(())
}
```

## Shutdown Behaviour

`Scheduler` implements [`modo::runtime::Task`](../runtime/trait.Task.html).
Calling `shutdown()` (or passing the scheduler to the `run!` macro) signals
all job loops to stop and waits up to **30 seconds** for any in-flight
executions to finish before returning.

Concurrency note: if a job execution is still running when the next tick
fires, the scheduler skips that tick and logs a warning rather than spawning
a second concurrent instance.
