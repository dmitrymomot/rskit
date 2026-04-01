//! # modo::db
//!
//! Lightweight libsql (SQLite) database layer with typed row mapping,
//! composable query building, filtering, and pagination.
//!
//! Requires feature `"db"`.
//!
//! ## Core types
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Database`] | Clone-able, `Arc`-wrapped single-connection handle |
//! | [`Config`] | YAML-deserializable database configuration with PRAGMA defaults |
//! | [`ManagedDatabase`] | Wrapper for graceful shutdown via [`crate::run!`] |
//! | [`managed`] | Wraps a [`Database`] into a [`ManagedDatabase`] |
//!
//! ## Connection & querying
//!
//! | Item | Purpose |
//! |------|---------|
//! | [`connect`] | Open a database, apply PRAGMAs, optionally run migrations |
//! | [`migrate`] | Run `*.sql` migrations from a directory with checksum tracking |
//! | [`ConnExt`] | Low-level `query_raw`/`execute_raw` trait for `Connection` and `Transaction` |
//! | [`ConnQueryExt`] | High-level `query_one`/`query_all`/`query_optional` helpers (blanket impl on `ConnExt`) |
//! | [`SelectBuilder`] | Composable query builder combining filters, sorting, and pagination |
//!
//! ## Row mapping
//!
//! | Item | Purpose |
//! |------|---------|
//! | [`FromRow`] | Trait for converting a `libsql::Row` into a Rust struct |
//! | [`FromValue`] | Trait for converting a `libsql::Value` into a concrete Rust type |
//! | [`ColumnMap`] | Column name to index lookup for name-based row access |
//!
//! ## Filtering & pagination
//!
//! | Item | Purpose |
//! |------|---------|
//! | [`Filter`] | Raw parsed filter from query string (axum extractor) |
//! | [`FilterSchema`] | Declares allowed filter and sort fields for an endpoint |
//! | [`ValidatedFilter`] | Schema-validated filter safe for SQL generation |
//! | [`FieldType`] | Column type enum for filter value validation |
//! | [`PageRequest`] | Offset-based pagination extractor (`?page=N&per_page=N`) |
//! | [`Page`] | Offset-based page response with total/has_next/has_prev |
//! | [`CursorRequest`] | Cursor-based pagination extractor (`?after=<cursor>&per_page=N`) |
//! | [`CursorPage`] | Cursor-based page response with next_cursor/has_more |
//! | [`PaginationConfig`] | Configurable defaults and limits for pagination extractors |
//!
//! ## Configuration enums
//!
//! | Enum | Purpose |
//! |------|---------|
//! | [`JournalMode`] | SQLite journal mode (WAL, Delete, Truncate, Memory, Off) |
//! | [`SynchronousMode`] | SQLite synchronous mode (Off, Normal, Full, Extra) |
//! | [`TempStore`] | SQLite temp store location (Default, File, Memory) |
//!
//! ## Re-exports
//!
//! The [`libsql`] crate is re-exported for direct access to low-level types
//! such as `libsql::params!`, `libsql::Value`, `libsql::Connection`, and
//! `libsql::Transaction`.
//!
//! ## Quick start
//!
//! ```rust,ignore
//! use modo::db::{self, ConnExt, ConnQueryExt};
//!
//! // Connect with defaults (data/app.db, WAL mode, FK on)
//! let db = db::connect(&db::Config::default()).await?;
//!
//! // Use query helpers via ConnQueryExt
//! let user: User = db.conn().query_one(
//!     "SELECT id, name FROM users WHERE id = ?1",
//!     libsql::params!["user_abc"],
//! ).await?;
//!
//! // Or use the SelectBuilder for filtered, paginated queries
//! let page = db.conn()
//!     .select("SELECT id, name FROM users")
//!     .filter(validated_filter)
//!     .order_by("\"created_at\" DESC")
//!     .page::<User>(page_request)
//!     .await?;
//! ```

mod error;

mod config;
pub use config::{Config, JournalMode, PoolConfig, SynchronousMode, TempStore};

mod database;
pub use database::Database;

mod connect;
pub use connect::connect;

mod from_row;
pub use from_row::{ColumnMap, FromRow, FromValue};

mod conn;
pub use conn::{ConnExt, ConnQueryExt};

mod managed;
pub use managed::{ManagedDatabase, managed};

mod pool;
pub use pool::{DatabasePool, ManagedDatabasePool, managed_pool};

mod migrate;
pub use migrate::migrate;

mod page;
pub use page::{CursorPage, CursorRequest, Page, PageRequest, PaginationConfig};

mod filter;
pub use filter::{FieldType, Filter, FilterSchema, ValidatedFilter};

mod select;
pub use select::SelectBuilder;

// Re-export libsql for direct access
pub use libsql;
