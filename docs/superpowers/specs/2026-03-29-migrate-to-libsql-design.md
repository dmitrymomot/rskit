# Migrate from sqlx to libsql

**Date:** 2026-03-29
**Status:** Draft
**Breaking:** Yes — no backward compatibility

## Goal

Replace sqlx with libsql as modo's sole database backend. Delete `src/db/`, `src/page/`, `src/domain_signup/`. Rename `src/ldb/` to `src/db/`. Port all consuming modules. No tech debt, no dual backends, no compatibility shims.

## Connection Model

Single `Database` type — Arc-wrapped single `libsql::Connection`. No connection pools, no `ReadPool`/`WritePool`, no `Reader`/`Writer` traits.

Rationale: benchmarks show single connection outperforms connection pooling with libsql. SQLite is fundamentally single-writer; the sqlx pool's concurrency was mostly about not blocking reads while waiting for the write lock. With WAL mode and a single connection, libsql handles this efficiently.

## Feature Flags

Transitive feature flags — enabling a dependent module auto-enables `db`:

```toml
[features]
default = ["db"]
db = ["dep:libsql", "dep:urlencoding"]
session = ["db"]
job = ["db"]
test-helpers = ["db"]
full = ["db", "session", "job", ...]  # now includes db (no linker conflict)
```

Other db-dependent modules (`health` checks) use `#[cfg(feature = "db")]` inline.

## Migration Steps

### Step 1: Core — delete old modules, rename ldb to db

- Delete `src/db/` (7 files + README)
- Delete `src/page/` (4 files: mod.rs, offset.rs, cursor.rs, value.rs)
- Delete `src/domain_signup/` (4 files: mod.rs, types.rs, registry.rs, validate.rs)
- Rename `src/ldb/` to `src/db/`
- Update all `crate::ldb::` references to `crate::db::`
- Cargo.toml:
  - Remove `sqlx` dependency
  - Remove old `db = ["dep:sqlx"]` and `ldb = ["dep:libsql", "dep:urlencoding"]` feature flags
  - New `db = ["dep:libsql", "dep:urlencoding"]` as default
  - Add transitive flags: `session = ["db"]`, `job = ["db"]`, `test-helpers = ["db"]`
  - Update `full` feature to include `db` (no longer excluded due to linker conflicts)
  - Remove `connect_rw()` — no split pools, single `connect()` only
- lib.rs:
  - Single `#[cfg(feature = "db")] pub mod db;`
  - Remove `pub use sqlx`
  - Remove `pub mod page` and `pub mod ldb` declarations
  - Remove `pub mod domain_signup`
- Fix cursor ordering gap in `SelectBuilder` — add direction control so cursor pagination supports both newest-first and oldest-first ordering

### Step 2: Port session module

- `Store` takes `Database` instead of separate `InnerPool`/`Reader`/`Writer`
- Replace `#[derive(sqlx::FromRow)]` with manual `FromRow` trait impl for `SessionRow`
- Replace all `sqlx::query`/`sqlx::query_as` with `ConnQueryExt` methods
- Remove transaction from `create()`:
  - Insert session first
  - Trim excess sessions with a second query: `DELETE FROM sessions WHERE id IN (SELECT id FROM sessions WHERE user_id = ? ORDER BY last_active_at DESC LIMIT -1 OFFSET ?)`
- Replace `sqlx::Error::Database` unique violation checks with libsql error equivalents
- Feature gate: `#[cfg(feature = "session")]`

### Step 3: Port job module

- `Enqueuer`, `Worker`, reaper, cleanup all take `Database`
- Designed for a separate `Database` instance (dedicated db for jobs)
- Replace all `sqlx::query`/`sqlx::query_as` with `ConnQueryExt` methods
- Replace `sqlx::Error::Database` unique violation detection with libsql error equivalents (for `enqueue_unique`)
- `UPDATE ... RETURNING` for job claiming — libsql supports this (SQLite 3.35+)
- Feature gate: `#[cfg(feature = "job")]`

### Step 4: Port tenant — absorb domain_signup as DomainService

- Delete `src/domain_signup/` as standalone module (already done in step 1)
- Create `src/tenant/domain.rs` with `DomainService` struct
- `DomainService::new(db: Database)` — holds Database, registered in service registry
- Extracted in handlers via `Service<DomainService>`

Domain ownership model — verify once, use for multiple purposes:

- Each domain claim has capability flags: `use_for_email: bool`, `use_for_routing: bool`
- Verification is domain-level (DNS TXT record at `_modo-verify.{domain}`)
- Once verified, capabilities can be toggled independently without re-verification
- Both flags can be true simultaneously

DomainService methods:
- `register(tenant_id, domain)` — create pending claim
- `verify(id)` — check DNS, transition to verified
- `remove(id)` — delete claim
- `enable_email(id)` / `disable_email(id)` — toggle email auto-join capability
- `enable_routing(id)` / `disable_routing(id)` — toggle custom domain routing capability
- `lookup_email_domain(email)` — find tenant by email domain (checks `use_for_email`)
- `lookup_routing_domain(domain)` — find tenant by custom domain (checks `use_for_routing`)
- `resolve_tenant(domain)` — resolve custom domain to tenant_id (for use as `TenantResolver` backend)
- `list(tenant_id)` — list all claims for tenant

Tenant resolution integration: `DomainService` provides `resolve_tenant(domain) -> Result<Option<String>>` which returns the `tenant_id` for a verified domain with `use_for_routing = true`. This is the lookup backend that a `TenantResolver` implementation uses when handling `TenantId::Domain` from the domain/subdomain_or_domain strategies.

Port all sqlx queries to `ConnQueryExt`. Replace `sqlx::FromRow` with ldb's `FromRow`. No dedicated feature flag — `DomainService` is opt-in via service registration; the tenant module itself is always available, and `DomainService` requires `db` at the call site.

### Step 5: Port health and testing modules

**Health:**
- Remove `db::Pool`, `db::ReadPool`, `db::WritePool` health check impls
- Add `Database` health check impl — execute `SELECT 1` via `ConnExt`
- Keep behind `#[cfg(feature = "db")]`

**Testing:**
- `TestDb` wraps `Database` instead of `Pool`
- Remove `read_pool()` / `write_pool()` methods — single `.db()` returning `&Database`
- `TestSession` updated to use `Database` and new session `Store` API
- Feature gate: `test-helpers` depends on `db`

## Types Removed

- `Pool`, `ReadPool`, `WritePool` (pool newtypes)
- `InnerPool` (type alias for `sqlx::SqlitePool`)
- `Reader`, `Writer` (traits)
- `PoolOverrides` (per-pool config)
- `ManagedPool` (shutdown wrapper for pools)
- `Paginate`, `CursorPaginate` (sqlx pagination builders)
- `SqliteValue`, `IntoSqliteValue` (sqlx bind parameter types)
- `DomainRegistry` (replaced by `DomainService`)

## Types Retained (from ldb, now in db)

- `Database` — Arc-wrapped single connection
- `Config` — YAML-deserializable configuration
- `JournalMode`, `SynchronousMode`, `TempStore` — PRAGMA enums
- `connect()` — open database, apply PRAGMAs
- `FromRow`, `FromValue`, `ColumnMap` — row deserialization
- `ConnExt`, `ConnQueryExt` — query helpers
- `ManagedDatabase`, `managed()` — graceful shutdown
- `migrate()` — migration runner with checksums
- `SelectBuilder` — composable query builder
- `Filter`, `FilterSchema`, `ValidatedFilter`, `FieldType` — request filtering
- `PageRequest`, `Page`, `CursorRequest`, `CursorPage`, `PaginationConfig` — pagination
- `libsql` — re-exported for direct access

## Dependencies

Remove from Cargo.toml:
- `sqlx`

Keep (already present for ldb):
- `libsql`
- `urlencoding`

## Error Handling

`From<libsql::Error>` for `modo::Error` already exists in ldb's `error.rs`. This becomes the sole database error conversion. Unique violation detection uses libsql's error variants instead of `sqlx::Error::Database`.

## Testing Strategy

Each step must compile and pass `cargo test --features <relevant>`. Integration tests that used sqlx types need updating. Test fixtures in `tests/fixtures/migrations/` remain unchanged (plain SQL files).
