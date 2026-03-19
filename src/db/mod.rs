mod config;
mod connect;
mod error;
mod migrate;
mod pool;

#[cfg(feature = "sqlite")]
pub use config::SqliteConfig;
pub use config::{JournalMode, PoolOverrides, SynchronousMode, TempStore};
#[cfg(feature = "sqlite")]
pub use connect::{connect, connect_rw};
pub use migrate::migrate;
pub use pool::{AsPool, InnerPool, Pool, ReadPool, WritePool};

#[cfg(feature = "sqlite")]
pub type Config = SqliteConfig;
