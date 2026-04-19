//! # modo::job
//!
//! Durable background job processing backed by SQLite.
//!
//! The `job` module provides a named-job queue stored in the `jobs` SQLite
//! table. Handlers are plain `async fn` functions. The worker polls the
//! database, dispatches jobs to handlers, retries failures with exponential
//! backoff, and reaps stale jobs.
//!
//! ## Priority model
//!
//! Job priority is expressed via **separate named queues** with independent
//! concurrency limits — there is no numeric priority field. Declare multiple
//! [`QueueConfig`] entries (e.g. `"critical"`, `"default"`, `"low"`) and
//! enqueue jobs onto the queue that matches their urgency. Workers drain every
//! configured queue on every poll tick, so a busy low-priority queue cannot
//! starve high-priority queues.
//!
//! ## Capabilities
//!
//! - Named jobs with JSON payloads serialized via `serde`
//! - Priority via separate named queues, each with its own concurrency limit
//! - Automatic retries with exponential backoff
//! - Scheduled execution via [`EnqueueOptions::run_at`]
//! - Idempotent enqueueing via [`Enqueuer::enqueue_unique`]
//! - Stale job reaping (jobs stuck in `running` beyond a configurable threshold)
//! - Job cancellation via [`Enqueuer::cancel`]
//! - Periodic cleanup of terminal jobs
//! - Optional separate SQLite database for job-queue isolation
//!
//! ## Provides
//!
//! **Configuration:**
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`JobConfig`] | Top-level worker configuration (poll interval, queues, cleanup, optional separate DB) |
//! | [`QueueConfig`] | Name and concurrency limit for a single queue |
//! | [`CleanupConfig`] | Interval and retention window for terminal-job cleanup |
//!
//! **Enqueueing:**
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Enqueuer`] | Inserts and cancels jobs in the `jobs` table |
//! | [`EnqueueOptions`] | Queue name and optional scheduled `run_at` timestamp |
//! | [`EnqueueResult`] | `Created(id)` or `Duplicate(id)` from idempotent enqueue |
//!
//! **Worker:**
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Worker`] | Running worker handle; implements [`crate::runtime::Task`] for graceful shutdown |
//! | [`WorkerBuilder`] | Fluent builder for registering handlers and starting the worker |
//! | [`JobOptions`] | Per-handler max-attempts and timeout |
//!
//! **Handler system:**
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`JobHandler`] | Trait blanket-implemented for `async fn`s with 0-12 [`FromJobContext`] args |
//! | [`JobContext`] | Runtime context carrying payload, metadata, and service registry |
//! | [`FromJobContext`] | Extraction trait for handler argument types |
//! | [`Payload`] | Handler argument — deserializes the JSON payload into `T` |
//! | [`Meta`] | Handler argument — job ID, name, queue, attempt count, deadline |
//! | [`Status`] | Job lifecycle enum: `Pending`, `Running`, `Completed`, `Dead`, `Cancelled` |
//!
//! ## Quick start
//!
//! ```rust,ignore
//! use modo::job::{Enqueuer, JobConfig, Payload, Meta, Worker};
//! use modo::service::Registry;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize)]
//! struct WelcomePayload { user_id: String }
//!
//! async fn send_welcome(p: Payload<WelcomePayload>, m: Meta) -> modo::Result<()> {
//!     tracing::info!(job_id = %m.id, user_id = %p.user_id, "sending email");
//!     Ok(())
//! }
//!
//! // Build and start the worker
//! let worker = Worker::builder(&config, &registry)
//!     .register("send_welcome", send_welcome)
//!     .start()
//!     .await;
//!
//! // Enqueue a job
//! let enqueuer = Enqueuer::new(db);
//! enqueuer.enqueue("send_welcome", &WelcomePayload {
//!     user_id: "usr_01".into(),
//! }).await?;
//! ```
//!
//! ## Database schema
//!
//! The module reads and writes the `jobs` table. End-applications own the
//! migration — no embedded migration is shipped by this module.
//!
//! ## Shutdown
//!
//! [`Worker`] implements [`crate::runtime::Task`] so it integrates with the
//! [`run!`](crate::run) macro for graceful shutdown.

mod cleanup;
mod config;
mod context;
mod enqueuer;
mod handler;
mod meta;
mod payload;
mod reaper;
mod worker;

pub use config::{CleanupConfig, JobConfig, QueueConfig};
pub use context::FromJobContext;
pub use context::JobContext;
pub use enqueuer::{EnqueueOptions, EnqueueResult, Enqueuer};
pub use handler::JobHandler;
pub use meta::{Meta, Status};
pub use payload::Payload;
pub use worker::{JobOptions, Worker, WorkerBuilder};
