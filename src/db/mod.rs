//! SQLite database layer for modo.
//!
//! This module provides connection pooling, migration support, and type-safe
//! pool wrappers built on top of [`sqlx`]. It supports two connection modes:
//!
//! - **Single pool** ([`connect`]) — one [`Pool`] for both reads and writes.
//!   Use this for simple apps or in-memory databases.
//! - **Read/write split** ([`connect_rw`]) — separate [`ReadPool`] and
//!   [`WritePool`] for workloads that benefit from concurrent readers and a
//!   single serialized writer. Not supported for `:memory:` databases.
//!
//! # Quick start
//!
//! ```no_run
//! # async fn example() -> modo::Result<()> {
//! use modo::db::{self, SqliteConfig};
//!
//! let config = SqliteConfig::default();
//! let pool = db::connect(&config).await?;
//! db::migrate("migrations", &pool).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Graceful shutdown
//!
//! Wrap a pool in [`ManagedPool`] via [`managed`] to integrate with the
//! `run!` macro shutdown sequence:
//!
//! ```ignore
//! let managed = db::managed(pool.clone());
//! run!(server, managed);
//! ```

mod config;
mod connect;
mod error;
mod managed;
mod migrate;
mod pool;

pub use config::SqliteConfig;
pub use config::{JournalMode, PoolOverrides, SynchronousMode, TempStore};
pub use connect::{connect, connect_rw};
pub use managed::{ManagedPool, managed};
pub use migrate::migrate;
pub use pool::{InnerPool, Pool, ReadPool, Reader, WritePool, Writer};

/// Type alias for [`SqliteConfig`].
pub type Config = SqliteConfig;
