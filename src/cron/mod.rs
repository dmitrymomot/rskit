//! Cron job scheduling for the modo framework.
//!
//! This module provides a cron scheduler that runs async handler functions on
//! configurable schedules. Handlers are plain async functions — no macros or
//! custom derives required. Services are injected via the
//! [`crate::service::Registry`] snapshot captured at scheduler build time.
//!
//! # Provides
//!
//! - [`Scheduler`] — running scheduler handle; implements
//!   [`Task`](crate::runtime::Task) for clean shutdown.
//! - [`SchedulerBuilder`] — builder returned by [`Scheduler::builder`].
//! - [`CronOptions`] — per-job options (timeout).
//! - [`Meta`] — job metadata injected into handler arguments.
//! - [`CronContext`] — full execution context passed to handlers.
//! - [`CronHandler`] — trait implemented automatically for matching `async fn`.
//! - [`FromCronContext`] — trait for types extractable from [`CronContext`].
//!
//! # Schedule formats
//!
//! Three formats are accepted wherever a schedule string is required:
//!
//! - **Standard cron expression** — 5-field (`"*/5 * * * *"`) or 6-field with a
//!   leading seconds field (`"0 30 9 * * *"`).
//! - **Named aliases** — `@yearly`, `@annually`, `@monthly`, `@weekly`,
//!   `@daily`, `@midnight`, `@hourly`.
//! - **Interval** — `@every <duration>` where duration is composed of `h`,
//!   `m`, and `s` units, e.g. `@every 5m`, `@every 1h30m`.
//!
//! Invalid expressions or durations return an error at scheduler build time.
//!
//! # Usage
//!
//! ```rust,no_run
//! use modo::cron::Scheduler;
//! use modo::service::Registry;
//! use modo::runtime::Task;
//! use modo::Result;
//!
//! async fn cleanup() -> Result<()> {
//!     Ok(())
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let registry = Registry::new();
//!
//!     let scheduler = Scheduler::builder(&registry)
//!         .job("@daily", cleanup).unwrap()
//!         .start()
//!         .await;
//!
//!     scheduler.shutdown().await.unwrap();
//! }
//! ```

mod context;
mod handler;
mod meta;
mod schedule;
mod scheduler;

pub use context::CronContext;
pub use context::FromCronContext;
pub use handler::CronHandler;
pub use meta::Meta;
pub use scheduler::{CronOptions, Scheduler, SchedulerBuilder};
