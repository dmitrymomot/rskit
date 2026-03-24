# modo::runtime

Graceful shutdown runtime for modo applications.

The module provides three composable pieces for orderly process teardown:

- `Task` trait — implement on any service that owns resources needing cleanup.
- `wait_for_shutdown_signal()` — async function that resolves on `SIGINT`/`SIGTERM`.
- `run!` macro — waits for the signal then shuts down all tasks in order.

## Key Types

| Item                       | Kind       | Description                                                |
| -------------------------- | ---------- | ---------------------------------------------------------- |
| `Task`                     | trait      | A service that can be shut down; consumed by `run!`        |
| `wait_for_shutdown_signal` | `async fn` | Resolves on Ctrl+C (all platforms) or SIGTERM (Unix)       |
| `run!`                     | macro      | Awaitable shutdown sequencer for one or more `Task` values |

## Usage

### Implementing Task

```rust
use modo::runtime::Task;
use modo::Result;

struct MyWorker {
    // handle, channel sender, etc.
}

impl Task for MyWorker {
    async fn shutdown(self) -> Result<()> {
        // flush buffers, close connections, etc.
        Ok(())
    }
}
```

### Running the application

Pass all tasks to `run!` and `.await` the result in `main`. Tasks are shut down
in the order they appear in the macro invocation.

```rust,no_run
use modo::runtime::Task;
use modo::Result;

struct HttpServer;
struct BackgroundWorker;

impl Task for HttpServer {
    async fn shutdown(self) -> Result<()> { Ok(()) }
}

impl Task for BackgroundWorker {
    async fn shutdown(self) -> Result<()> { Ok(()) }
}

#[tokio::main]
async fn main() -> Result<()> {
    let server = HttpServer;
    let worker = BackgroundWorker;

    // Blocks until SIGINT/SIGTERM, then shuts down server first, worker second.
    modo::run!(server, worker).await
}
```

### Using the signal function directly

`wait_for_shutdown_signal` is also available on its own when you need custom
shutdown logic without the `run!` macro.

```rust,no_run
use modo::runtime::wait_for_shutdown_signal;

#[tokio::main]
async fn main() {
    wait_for_shutdown_signal().await;
    println!("shutting down...");
}
```

## Signal Handling

| Platform | Signals handled              |
| -------- | ---------------------------- |
| Unix     | `SIGINT` (Ctrl+C), `SIGTERM` |
| Non-Unix | `SIGINT` (Ctrl+C) only       |

## Notes

- `run!` logs each step with `tracing::info!` and logs shutdown errors with
  `tracing::error!` — configure a tracing subscriber in `main` before calling it.
- A task that returns `Err` from `shutdown` does **not** abort the remaining
  tasks; all tasks are always attempted.
- `Task::shutdown` consumes `self`, so a task cannot be reused after shutdown.
