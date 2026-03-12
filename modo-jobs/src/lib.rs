//! Background job processing for the modo framework.
//!
//! `modo-jobs` provides persistent, database-backed job queues with:
//!
//! - Compile-time job registration via the `#[job]` attribute macro
//! - Per-queue concurrency limits and configurable polling intervals
//! - Automatic retries with exponential backoff
//! - Scheduled execution via `enqueue_at`
//! - In-memory cron scheduling (not persisted to the database)
//! - Graceful shutdown with configurable drain timeout
//!
//! Jobs are defined as async functions annotated with `#[job]` and are
//! automatically registered at link time using the `inventory` crate.
//! No explicit registration call is needed at startup.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use modo_jobs::job;
//! use modo::HandlerResult;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize)]
//! struct WelcomePayload {
//!     email: String,
//! }
//!
//! #[job(queue = "default")]
//! async fn send_welcome(payload: WelcomePayload) -> HandlerResult<()> {
//!     tracing::info!(email = %payload.email, "Sending welcome email");
//!     Ok(())
//! }
//! ```
//!
//! Start the runner in `main` and register the handle as a managed service:
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let db = modo_db::connect(&Default::default()).await?;
//! let jobs = modo_jobs::new(&db, &Default::default())
//!     .service(db.clone())
//!     .run()
//!     .await?;
//!
//! // app.managed_service(jobs) registers for graceful shutdown
//! # Ok(())
//! # }
//! ```

pub mod config;
pub(crate) mod cron;
pub mod entity;
pub mod extractor;
pub mod handler;
pub mod queue;
pub mod runner;
pub mod types;

// Public API
pub use config::{CleanupConfig, JobsConfig, QueueConfig};
pub use handler::{JobContext, JobHandler, JobHandlerDyn, JobRegistration};
pub use queue::JobQueue;
pub use runner::{JobsBuilder, JobsHandle, new};
pub use types::{JobId, JobState};

// Re-export proc macros
pub use modo_jobs_macros::job;

// Re-exports for macro-generated code
pub use chrono;
pub use inventory;
pub use modo;
pub use modo_db;
pub use serde_json;
