//! # modo::db
//!
//! Lightweight libsql (SQLite) database layer with typed row mapping,
//! composable query building, filtering, and pagination.
//!
//! Provides:
//!
//! - A single-connection [`Database`] handle (cheap-to-clone `Arc<Inner>`
//!   over one `libsql::Connection`).
//! - [`connect`] — opens **one** connection, applies PRAGMAs, runs migrations.
//! - [`ConnExt`] — low-level `query_raw`/`execute_raw` on `libsql::Connection`
//!   and `libsql::Transaction`.
//! - [`ConnQueryExt`] — typed helpers (`query_one`, `query_optional`,
//!   `query_all` and their `_map` closure variants), blanket-implemented on
//!   every `ConnExt`.
//! - [`SelectBuilder`] for composable `WHERE`/`ORDER BY`/pagination.
//! - [`Filter`] / [`FilterSchema`] / [`ValidatedFilter`] — schema-checked
//!   filtering from query strings.
//! - Offset ([`Page`] / [`PageRequest`]) and cursor ([`CursorPage`] /
//!   [`CursorRequest`]) pagination.
//! - [`FromRow`] / [`FromValue`] / [`ColumnMap`] for row mapping.
//! - [`DatabasePool`] — lazy multi-database pool for tenant sharding.
//! - Maintenance: [`DbHealth`], [`run_vacuum`], [`vacuum_if_needed`],
//!   [`vacuum_handler`].
//! - [`migrate`] — idempotent SQL migration runner with checksum tracking.
//! - [`ManagedDatabase`] / [`ManagedDatabasePool`] for graceful shutdown.
//!
//! ## Core types
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`Database`] | Clone-able, `Arc`-wrapped single-connection handle |
//! | [`Config`] | YAML-deserializable database configuration with PRAGMA defaults |
//! | [`ManagedDatabase`] | Wrapper for graceful shutdown via `modo::run!` |
//! | [`DatabasePool`] | Multi-database pool with lazy shard opening for tenant isolation |
//! | [`PoolConfig`] | Configuration for database sharding (`base_path`, `lock_shards`) |
//! | [`ManagedDatabasePool`] | Wrapper for graceful pool shutdown via `modo::run!` |
//!
//! ## Factory functions
//!
//! | Function | Purpose |
//! |----------|---------|
//! | [`managed`] | Wraps a [`Database`] into a [`ManagedDatabase`] |
//! | [`managed_pool`] | Wraps a [`DatabasePool`] into a [`ManagedDatabasePool`] |
//!
//! ## Connection & querying
//!
//! | Item | Purpose |
//! |------|---------|
//! | [`connect`] | Open a database, apply PRAGMAs, optionally run migrations |
//! | [`migrate`] | Run `*.sql` migrations from a directory with checksum tracking |
//! | [`ConnExt`] | Low-level `query_raw`/`execute_raw` trait for `Connection` and `Transaction` |
//! | [`ConnQueryExt`] | High-level `query_one`/`query_all`/`query_optional` + `_map` variants (blanket impl on `ConnExt`) |
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
//! ## Maintenance
//!
//! | Item | Purpose |
//! |------|---------|
//! | [`DbHealth`] | Page-level health metrics from PRAGMA introspection |
//! | [`VacuumOptions`] | Configuration for [`run_vacuum`] (threshold, dry_run) |
//! | [`VacuumResult`] | Before/after health snapshots with timing |
//! | [`run_vacuum`] | VACUUM with threshold guard and health snapshots |
//! | [`vacuum_if_needed`] | Shorthand for `run_vacuum` with threshold only |
//! | [`vacuum_handler`] | Cron handler factory for scheduled maintenance |
//!
//! ## Re-exports
//!
//! The [`libsql`] crate is re-exported for direct access to low-level types
//! such as `libsql::params!`, `libsql::Value`, `libsql::Connection`, and
//! `libsql::Transaction`.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use modo::db::{self, ConnExt, ConnQueryExt, ColumnMap, FromRow};
//!
//! struct User {
//!     id: String,
//!     name: String,
//! }
//!
//! impl FromRow for User {
//!     fn from_row(row: &libsql::Row) -> modo::Result<Self> {
//!         let cols = ColumnMap::from_row(row);
//!         Ok(Self {
//!             id: cols.get(row, "id")?,
//!             name: cols.get(row, "name")?,
//!         })
//!     }
//! }
//!
//! # async fn example() -> modo::Result<()> {
//! // Connect with defaults (data/app.db, WAL mode, FK on) — opens ONE
//! // underlying libsql::Connection wrapped in an Arc.
//! let db = db::connect(&db::Config::default()).await?;
//!
//! // Use typed helpers via ConnQueryExt (blanket-implemented on ConnExt).
//! let user: User = db
//!     .conn()
//!     .query_one(
//!         "SELECT id, name FROM users WHERE id = ?1",
//!         libsql::params!["user_abc"],
//!     )
//!     .await?;
//!
//! // Or use the SelectBuilder for filtered, ordered, paginated queries.
//! let users: Vec<User> = db
//!     .conn()
//!     .select("SELECT id, name FROM users")
//!     .order_by("\"name\" ASC")
//!     .fetch_all()
//!     .await?;
//! # Ok(()) }
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

mod maintenance;
pub use maintenance::{
    DbHealth, VacuumOptions, VacuumResult, run_vacuum, vacuum_handler, vacuum_if_needed,
};

// Re-export libsql for direct access
pub use libsql;
