//! Database integration for the modo framework.
//!
//! Provides SeaORM-backed connection pooling, schema synchronisation, versioned
//! migrations, pagination helpers, and a compile-time entity/migration
//! registration system built on [`inventory`].
//!
//! # Features
//!
//! - `sqlite` *(default)* — enables SQLite support via `sqlx-sqlite`.
//! - `postgres` — enables PostgreSQL support via `sqlx-postgres`.
//!
//! # Quick start
//!
//! ```rust,ignore
//! #[modo_db::entity(table = "todos")]
//! #[entity(timestamps)]
//! pub struct Todo {
//!     #[entity(primary_key, auto = "ulid")]
//!     pub id: String,
//!     pub title: String,
//! }
//!
//! // Entity in a named group (synced separately)
//! #[modo_db::entity(table = "analytics", group = "analytics")]
//! pub struct Event {
//!     #[entity(primary_key, auto = "ulid")]
//!     pub id: String,
//!     pub name: String,
//! }
//!
//! #[modo::main]
//! async fn main(
//!     app: modo::app::AppBuilder,
//!     config: Config,
//! ) -> Result<(), Box<dyn std::error::Error>> {
//!     let db = modo_db::connect(&config.database).await?;
//!     modo_db::sync_and_migrate(&db).await?;           // syncs all entities
//!     // modo_db::sync_and_migrate_group(&other_db, "analytics").await?;  // syncs only "analytics" group
//!     app.config(config.core).managed_service(db).run().await
//! }
//! ```

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
pub use sync::{sync_and_migrate, sync_and_migrate_group};

// Re-export proc macros
pub use modo_db_macros::{entity, migration};

// Re-exports for macro-generated code
pub use async_trait;
pub use chrono;
pub use inventory;
pub use sea_orm;
