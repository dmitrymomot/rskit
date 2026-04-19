# modo::cron

Periodic cron job scheduling with plain `async fn` handlers.

Handlers are plain `async fn` values — no macros, no derives. Services are
injected via the `Registry` snapshot captured at build time. The scheduler
integrates with the runtime's `run!` macro through the `Task` trait for
clean shutdown.

## Schedule formats

The following formats are accepted wherever a schedule string is required:

| Format                                   | Examples                                                                        |
| ---------------------------------------- | ------------------------------------------------------------------------------- |
| Standard cron (5-field)                  | `"*/5 * * * *"`, `"0 9 * * 1"`                                                  |
| Standard cron (6-field, leading seconds) | `"0 30 9 * * *"`, `"0 0 0 * * *"`                                               |
| Named alias                              | `@yearly`, `@annually`, `@monthly`, `@weekly`, `@daily`, `@midnight`, `@hourly` |
| Interval                                 | `@every 5m`, `@every 1h30m`, `@every 30s`                                       |

Invalid expressions or durations return an error at builder time (from
`SchedulerBuilder::job` / `SchedulerBuilder::job_with`).

## Key types

| Type                | Description                                                        |
| ------------------- | ------------------------------------------------------------------ |
| `Scheduler`         | Running scheduler handle; implements `Task` for shutdown           |
| `SchedulerBuilder`  | Builder returned by `Scheduler::builder()`                         |
| `CronOptions`       | Per-job options (timeout); default timeout is 300 s                |
| `Meta`              | Job metadata injected into handler arguments                       |
| `CronContext`       | Execution context carrying the registry snapshot and job metadata  |
| `CronHandler<Args>` | Trait implemented automatically for matching `async fn`            |
| `FromCronContext`   | Trait for types extractable from `CronContext`                     |

## Usage

### Basic scheduling

```rust,ignore
use modo::cron::Scheduler;
use modo::runtime::Task;
use modo::service::{Registry, Service};
use modo::Result;

struct EmailService;

async fn send_digest(svc: Service<EmailService>) -> Result<()> {
    // svc.0 is Arc<EmailService>
    Ok(())
}

async fn heartbeat() -> Result<()> {
    Ok(())
}

#[tokio::main]
async fn main() {
    let mut registry = Registry::new();
    registry.add(EmailService);

    let scheduler = Scheduler::builder(&registry)
        .job("@daily", send_digest).unwrap()
        .job("@every 1m", heartbeat).unwrap()
        .start()
        .await;

    // Integrate with the run! macro for graceful shutdown, or shut down manually:
    scheduler.shutdown().await.unwrap();
}
```

### Custom timeout

```rust,ignore
use modo::cron::{Scheduler, CronOptions};
use modo::runtime::Task;
use modo::service::Registry;
use modo::Result;

async fn slow_job() -> Result<()> {
    Ok(())
}

#[tokio::main]
async fn main() {
    let registry = Registry::new();

    let mut opts = CronOptions::default();
    opts.timeout_secs = 600;

    let scheduler = Scheduler::builder(&registry)
        .job_with("@hourly", slow_job, opts)
        .unwrap()
        .start()
        .await;

    scheduler.shutdown().await.unwrap();
}
```

### Accessing job metadata

Declare `Meta` as a handler argument to receive the job name, scheduled tick
time, and execution deadline:

```rust,ignore
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

## Shutdown behavior

`Scheduler` implements `modo::runtime::Task`. Calling `shutdown()` (or
passing the scheduler to the `run!` macro) signals all job loops to stop and
waits up to **30 seconds** for any in-flight executions to finish before
returning.

Concurrency note: if a job execution is still running when the next tick
fires, the scheduler skips that tick and logs a warning rather than spawning
a second concurrent instance.
