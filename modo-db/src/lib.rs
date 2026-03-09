pub mod config;
pub mod connect;
pub mod entity;
pub mod extractor;
pub mod id;
pub mod migration;
pub mod pagination;
pub mod pool;
pub mod sync;

// Public API
pub use config::DatabaseConfig;
pub use connect::connect;
pub use entity::EntityRegistration;
pub use extractor::Db;
pub use id::{generate_nanoid, generate_ulid};
pub use migration::MigrationRegistration;
pub use pagination::{
    CursorParams, CursorResult, PageParams, PageResult, paginate, paginate_cursor,
};
pub use pool::DbPool;
pub use sync::sync_and_migrate;

// Re-export proc macros
pub use modo_db_macros::{entity, migration};

// Re-exports for macro-generated code
pub use async_trait;
pub use chrono;
pub use inventory;
pub use sea_orm;
