//! # modo::runtime
//!
//! Graceful shutdown runtime for modo applications.
//!
//! Provides three composable building blocks for orderly process teardown:
//!
//! - [`Task`] — a trait for any service that can be shut down asynchronously.
//! - [`wait_for_shutdown_signal`] — async function that resolves on `SIGINT`
//!   (Ctrl+C) or, on Unix, `SIGTERM`.
//! - [`run!`](crate::run) — macro that waits for a signal and then calls
//!   [`Task::shutdown`] on each supplied value in declaration order.
//!
//! ## Quick start
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
//! ### Using `wait_for_shutdown_signal` directly
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
