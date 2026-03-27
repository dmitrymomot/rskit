//! Graceful shutdown runtime for the modo framework.
//!
//! This module provides three building blocks for orderly application teardown:
//!
//! - [`Task`] — a trait for any service that can be shut down asynchronously.
//! - [`wait_for_shutdown_signal`] — an async function that resolves when the process
//!   receives `SIGINT` (Ctrl+C) or, on Unix, `SIGTERM`.
//! - `run!` — a macro that waits for a shutdown signal and then calls
//!   [`Task::shutdown`] on each supplied value in declaration order.
//!
//! # Examples
//!
//! ## Using the `run!` macro
//!
//! ```rust,no_run
//! use modo::runtime::Task;
//! use modo::Result;
//!
//! struct MyServer;
//!
//! impl Task for MyServer {
//!     async fn shutdown(self) -> Result<()> {
//!         // perform graceful shutdown
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let server = MyServer;
//!     modo::run!(server).await
//! }
//! ```
//!
//! ## Using `wait_for_shutdown_signal` directly
//!
//! ```rust,no_run
//! use modo::runtime::wait_for_shutdown_signal;
//!
//! #[tokio::main]
//! async fn main() {
//!     wait_for_shutdown_signal().await;
//!     println!("shutting down...");
//! }
//! ```

mod run_macro;
mod signal;
mod task;

pub use signal::wait_for_shutdown_signal;
pub use task::Task;
