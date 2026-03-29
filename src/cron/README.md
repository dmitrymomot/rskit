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
| Standard cron (5-field)                  | `"*/5 * * * *"`, `"0 9 * * 1"`                                                  |
| Standard cron (6-field, leading seconds) | `"0 30 9 * * *"`, `"0 0 0 * * *"`                                               |
| Named alias                              | `@yearly`, `@annually`, `@monthly`, `@weekly`, `@daily`, `@midnight`, `@hourly` |
| Interval                                 | `@every 5m`, `@every 1h30m`, `@every 30s`                                       |

Invalid expressions or durations return an error at builder time.

## Key Types

| Type                | Description                                              |
| ------------------- | -------------------------------------------------------- |
| `Scheduler`         | Running scheduler handle; implements `Task` for shutdown |
| `SchedulerBuilder`  | Builder returned by `Scheduler::builder()`               |
| `CronOptions`       | Per-job options (timeout); default timeout is 300 s      |
| `Meta`              | Job metadata injected into handler arguments             |
| `CronContext`       | Full execution context; not used directly by handlers    |
| `CronHandler<Args>` | Trait implemented automatically for matching `async fn`  |
| `FromCronContext`   | Trait for types extractable from `CronContext`           |

## Basic Usage

```rust
use modo::cron::Scheduler;
use modo::extractor::Service;
use modo::runtime::Task;
use modo::service::Registry;
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

## Custom Timeout

```rust
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

    let scheduler = Scheduler::builder(&registry)
        .job_with(
            "@hourly",
            slow_job,
            CronOptions { timeout_secs: 600 },
        )
        .unwrap()
        .start()
        .await;

    scheduler.shutdown().await.unwrap();
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
