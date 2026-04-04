# Database Maintenance Module

**Module:** `db::maintenance`
**File:** `src/db/maintenance.rs`
**Feature flag:** `db` (existing, no new flag)
**Date:** 2026-04-04

---

## Problem

Bulk data deletions (tenant removal, expired data purges) leave SQLite/libsql databases with unreclaimable dead pages. Without periodic maintenance the database file grows monotonically, fragmentation increases, and sequential read performance degrades.

modo currently has no built-in way to assess database health (freelist ratio, file bloat) or safely run `VACUUM` with pre-checks.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Consumer model | Library functions + cron handler factory; end-app wires it | Matches modo's explicit-wiring philosophy |
| Feature flag | Existing `db` flag | Zero new dependencies, just PRAGMA queries and VACUUM |
| Connection type | `&libsql::Connection` | Consistent with `ConnExt`/`ConnQueryExt` pattern |
| File structure | Single flat file `src/db/maintenance.rs` | ~150 lines, YAGNI on sub-modules |
| Health scope | Page/freelist metrics only | `integrity_check` is a separate concern (expensive) |
| Logging | `debug` in core functions, `info` in cron handler | Composable for library callers, visible for operators |

## Public API

### `DbHealth`

```rust
/// Database health metrics from PRAGMA introspection.
#[derive(Debug, Clone)]
pub struct DbHealth {
    /// Total number of pages in the database.
    pub page_count: u64,
    /// Number of pages on the freelist (reclaimable by VACUUM).
    pub freelist_count: u64,
    /// Size of each page in bytes.
    pub page_size: u64,
    /// Percentage of pages on the freelist (0.0–100.0).
    pub free_percent: f64,
    /// Total database file size in bytes (page_count * page_size).
    pub total_size_bytes: u64,
    /// Wasted space in bytes (freelist_count * page_size).
    pub wasted_bytes: u64,
}
```

**Methods:**

- `async fn collect(conn: &libsql::Connection) -> Result<Self>` — runs `PRAGMA page_count`, `PRAGMA freelist_count`, `PRAGMA page_size`, computes derived fields.
- `fn needs_vacuum(&self, threshold_percent: f64) -> bool` — returns `true` if `free_percent >= threshold_percent`.

### `VacuumOptions`

```rust
pub struct VacuumOptions {
    /// Only vacuum if freelist exceeds this percentage. Default: 20.0
    pub threshold_percent: f64,
    /// Log-only mode — report health without running VACUUM. Default: false
    pub dry_run: bool,
}
```

`Default` impl: `threshold_percent: 20.0`, `dry_run: false`.

### `VacuumResult`

```rust
pub struct VacuumResult {
    /// Health snapshot taken before the vacuum decision.
    pub health_before: DbHealth,
    /// Health snapshot taken after VACUUM. `None` if skipped or dry_run.
    pub health_after: Option<DbHealth>,
    /// Whether VACUUM actually executed.
    pub vacuumed: bool,
    /// Wall-clock duration of the full operation.
    pub duration: std::time::Duration,
}
```

### Core Functions

```rust
/// Run VACUUM with safety checks.
///
/// 1. Collects health metrics.
/// 2. If free_percent < threshold or dry_run, returns early.
/// 3. Executes VACUUM.
/// 4. Collects health metrics again.
///
/// Logs before/after metrics at debug level.
pub async fn run_vacuum(
    conn: &libsql::Connection,
    opts: VacuumOptions,
) -> Result<VacuumResult>;

/// Shorthand: run_vacuum with the given threshold and defaults.
pub async fn vacuum_if_needed(
    conn: &libsql::Connection,
    threshold_percent: f64,
) -> Result<VacuumResult>;
```

### Cron Handler Factory

```rust
/// Returns a cron handler that checks DB health and vacuums if needed.
///
/// Extracts `Service<Database>` from the cron context. Logs at info level.
pub fn vacuum_handler(threshold_percent: f64) -> VacuumHandler;
```

Implementation: `VacuumHandler` is a private-fields, `#[derive(Clone)]` struct that implements `CronHandler<(Service<Database>,)>` manually. It calls `run_vacuum` with the captured threshold and logs results at `info` level.

## Module Wiring

**`src/db/mod.rs`** adds:

```rust
mod maintenance;
pub use maintenance::{
    DbHealth, VacuumOptions, VacuumResult,
    run_vacuum, vacuum_if_needed, vacuum_handler,
};
```

## End-App Usage

### On-demand after bulk deletes

```rust
use modo::db::maintenance;

// After tenant data deletion in a background job
let result = maintenance::vacuum_if_needed(db.conn(), 25.0).await?;
if result.vacuumed {
    tracing::info!(
        before_pct = result.health_before.free_percent,
        after_pct = result.health_after.as_ref().unwrap().free_percent,
        duration_ms = result.duration.as_millis(),
        "post-deletion vacuum completed"
    );
}
```

### Scheduled via cron

```rust
use modo::db::maintenance;
use modo::cron::Scheduler;

// Database must be registered in the service registry
registry.register(db.clone());

let scheduler = Scheduler::builder(&registry)
    .job("0 3 * * 0", maintenance::vacuum_handler(20.0))? // Weekly Sunday 3am
    .start()
    .await;
```

## Security

`DbHealth` exposes internal infrastructure metrics (page counts, file sizes, freelist ratios). **Do not serialize `DbHealth` on unauthenticated endpoints** like `/_ready` or `/_live`. The existing `/_ready` returns only a status code — keep it that way. If an app needs a health dashboard with vacuum metrics, gate it behind authentication.

`DbHealth` intentionally does **not** derive `Serialize` to prevent accidental exposure. Callers who need JSON output can map the fields explicitly into their own response type.

## Testing Strategy

Unit tests in `src/db/maintenance.rs` using in-memory libsql databases:

1. **`DbHealth::collect`** — create in-memory DB, verify page_count > 0, freelist_count == 0 on fresh DB, page_size == 4096 (default).
2. **`needs_vacuum`** — construct `DbHealth` manually, assert threshold logic.
3. **`run_vacuum` below threshold** — fresh DB has 0% free; verify `vacuumed == false`, `health_after == None`.
4. **`run_vacuum` dry_run** — even if threshold met, verify `vacuumed == false`.
5. **`run_vacuum` executes** — insert rows, delete them, verify freelist grows, run vacuum, verify freelist shrinks.
6. **`vacuum_if_needed`** — verify it delegates correctly (threshold pass-through).

The `VacuumHandler` struct is tested indirectly — it's a thin wrapper over `run_vacuum` with `Service<Database>` extraction.

## Operational Considerations

### Litestream

`VACUUM` rewrites the entire database file, forcing Litestream to re-replicate in full. A weekly schedule keeps this manageable. End-app operators should be aware of this trade-off.

### WAL Mode

`VACUUM` in WAL mode checkpoints first, then rewrites. This temporarily blocks writers. For single-server deployments with low traffic at maintenance time, this is a non-issue.

### libsql Compatibility

The implementation relies on:
- `PRAGMA page_count` — standard SQLite, expected to work in libsql.
- `PRAGMA freelist_count` — standard SQLite, expected to work in libsql.
- `PRAGMA page_size` — standard SQLite, expected to work in libsql.
- `VACUUM` — standard SQLite, expected to work in libsql embedded mode.

These should be verified against the actual libsql version during implementation.

### What This Doesn't Solve

- **Index fragmentation** — `REINDEX` is cheaper if only indexes are degraded.
- **WAL file growth** — managed by checkpoint policy, not vacuum.
- **Query performance** — likely missing indexes, not fragmentation.
