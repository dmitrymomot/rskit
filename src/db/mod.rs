mod config;
mod connect;
mod error;
mod managed;
mod migrate;
mod pool;

#[cfg(feature = "sqlite")]
pub use config::SqliteConfig;
#[cfg(feature = "sqlite")]
pub use config::{JournalMode, PoolOverrides, SynchronousMode, TempStore};
#[cfg(feature = "sqlite")]
pub use connect::{connect, connect_rw};
pub use managed::{ManagedPool, managed};
pub use migrate::migrate;
pub use pool::{InnerPool, Pool, ReadPool, Reader, WritePool, Writer};

#[cfg(feature = "sqlite")]
pub type Config = SqliteConfig;

#[cfg(feature = "postgres")]
pub use config::PostgresConfig;

#[cfg(feature = "postgres")]
pub type Config = PostgresConfig;
