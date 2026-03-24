//! Background job processing backed by SQLite.
//!
//! The `job` module provides a durable, SQLite-backed job queue with:
//!
//! - Named jobs with JSON payloads serialized via `serde`
//! - Per-queue concurrency limits
//! - Automatic retries with exponential backoff
//! - Scheduled execution via [`EnqueueOptions::run_at`]
//! - Idempotent enqueueing via [`Enqueuer::enqueue_unique`]
//! - Stale job reaping (jobs stuck in `running` beyond a configurable threshold)
//! - Periodic cleanup of terminal jobs
//!
//! # Database schema
//!
//! The module reads and writes the `modo_jobs` table. End-applications own the
//! migration — no embedded migration is shipped by this module.
//!
//! # Shutdown
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
