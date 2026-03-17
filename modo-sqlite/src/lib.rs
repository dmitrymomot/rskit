//! Pure sqlx SQLite layer for the modo framework.
//!
//! Provides connection pool management with optional read/write split,
//! configurable SQLite PRAGMAs, and embedded SQL migrations via `inventory`.

pub mod config;
pub mod error;
pub mod pool;

pub use config::{JournalMode, PoolOverrides, SqliteConfig, SynchronousMode, TempStore};
pub use error::Error;
pub use pool::{AsPool, Pool, ReadPool, WritePool};
