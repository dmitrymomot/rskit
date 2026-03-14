//! Procedural macros for `modo-jobs`.
//!
//! This crate is a proc-macro companion to `modo-jobs` and is not meant to be
//! used directly.  Import the macro through `modo_jobs::job` instead.

use proc_macro::TokenStream;

mod job;

/// Annotate an async function as a background job handler.
///
/// The macro generates:
/// - A unit struct `<FnName>Job` (PascalCase) that implements `JobHandler`.
/// - A `JOB_NAME: &str` constant on the struct holding the snake_case function name.
/// - `enqueue` and `enqueue_at` async methods on the struct (omitted for cron jobs).
/// - An `inventory` registration so the job is discovered automatically at startup.
///
/// # Parameters
///
/// | Parameter      | Type                      | Default     | Description                                                                            |
/// |----------------|---------------------------|-------------|----------------------------------------------------------------------------------------|
/// | `queue`        | string                    | `"default"` | Target queue name (must match a configured queue).                                     |
/// | `priority`     | integer                   | `0`         | Higher values run first within the same queue.                                         |
/// | `max_attempts` | integer                   | `3`         | Retry limit before the job is marked `dead`.                                           |
/// | `timeout`      | string (`"Xs"`/`"Xm"`/`"Xh"`) | `"5m"` | Per-execution timeout.                                                            |
/// | `cron`         | string (cron expression)  | —           | Schedule a recurring in-memory job. Mutually exclusive with `queue`, `priority`, and `max_attempts`. |
///
/// # Function Signature Rules
///
/// - The function must be `async`.
/// - At most one plain parameter is treated as the **payload** (deserialized from JSON).
/// - Use `Service<T>` to inject a registered service and `Db` to inject the database pool.
/// - Return type must be `Result<(), modo::Error>` (or any compatible alias, e.g. `HandlerResult<()>`).
///
/// # Examples
///
/// ```rust,ignore
/// use modo_jobs::job;
/// use modo::HandlerResult;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize)]
/// struct WelcomePayload { email: String }
///
/// // Regular queued job
/// #[job(queue = "default", max_attempts = 5, timeout = "30s")]
/// async fn send_welcome(payload: WelcomePayload) -> HandlerResult<()> {
///     tracing::info!(email = %payload.email, "Sending welcome email");
///     Ok(())
/// }
///
/// // Cron job — runs every minute, no payload, no enqueue methods
/// #[job(cron = "0 */1 * * * *", timeout = "10s")]
/// async fn heartbeat() -> HandlerResult<()> {
///     tracing::info!("heartbeat tick");
///     Ok(())
/// }
/// ```
///
/// The generated `SendWelcomeJob::enqueue` and `SendWelcomeJob::enqueue_at`
/// methods can then be called from HTTP handlers:
///
/// ```rust,ignore
/// let job_id = SendWelcomeJob::enqueue(&queue, &payload).await?;
/// let job_id = SendWelcomeJob::enqueue_at(&queue, &payload, run_at).await?;
/// ```
#[proc_macro_attribute]
pub fn job(attr: TokenStream, item: TokenStream) -> TokenStream {
    match job::expand(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
