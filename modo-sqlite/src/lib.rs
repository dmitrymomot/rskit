//! Pure sqlx SQLite layer for the modo framework.
//!
//! Provides connection pool management with optional read/write split,
//! configurable SQLite PRAGMAs, and embedded SQL migrations via `inventory`.

pub mod config;
pub mod connect;
pub mod error;
pub mod extractor;
pub mod id;
pub mod migration;
pub mod pool;

pub use config::{JournalMode, PoolOverrides, SqliteConfig, SynchronousMode, TempStore};
pub use connect::{connect, connect_rw};
pub use error::Error;
pub use extractor::{Db, DbReader, DbWriter};
pub use id::{generate_short_id, generate_ulid};
pub use migration::{
    MigrationRegistration, run_migrations, run_migrations_except, run_migrations_group,
};
pub use pool::{AsPool, Pool, ReadPool, WritePool};
