# Database Maintenance Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `db::maintenance` module with health check metrics, threshold-guarded VACUUM, and a cron handler factory.

**Architecture:** Single file `src/db/maintenance.rs` under the existing `db` feature flag. Library functions take `&libsql::Connection`. A `VacuumHandler` struct implements `CronHandler` for cron scheduling. Core functions log at `debug`, cron handler at `info`.

**Tech Stack:** libsql (PRAGMA queries, VACUUM), tracing, modo's cron system (`CronHandler`, `FromCronContext`, `Service<T>`)

**Spec:** `docs/superpowers/specs/2026-04-04-db-maintenance-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `src/db/maintenance.rs` | Create | All types (`DbHealth`, `VacuumOptions`, `VacuumResult`, `VacuumHandler`) and functions (`run_vacuum`, `vacuum_if_needed`, `vacuum_handler`) |
| `src/db/mod.rs` | Modify | Add `mod maintenance` + re-exports, update module doc table |

---

### Task 1: `DbHealth` struct and `collect`

**Files:**
- Create: `src/db/maintenance.rs`
- Modify: `src/db/mod.rs`

- [ ] **Step 1: Create `maintenance.rs` with `DbHealth` struct, `collect`, `needs_vacuum`, and a test**

Create `src/db/maintenance.rs`:

```rust
use crate::error::{Error, Result};

/// Database health metrics from PRAGMA introspection.
///
/// Contains page-level statistics useful for deciding whether to run
/// `VACUUM`. Does **not** derive `Serialize` — these are internal
/// infrastructure metrics that must not be exposed on unauthenticated
/// endpoints.
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
    /// Total database file size in bytes (`page_count * page_size`).
    pub total_size_bytes: u64,
    /// Wasted space in bytes (`freelist_count * page_size`).
    pub wasted_bytes: u64,
}

impl DbHealth {
    /// Collect health metrics via `PRAGMA page_count`, `freelist_count`,
    /// `page_size`. Computes derived fields from those three values.
    pub async fn collect(conn: &libsql::Connection) -> Result<Self> {
        let page_count = Self::pragma_u64(conn, "page_count").await?;
        let freelist_count = Self::pragma_u64(conn, "freelist_count").await?;
        let page_size = Self::pragma_u64(conn, "page_size").await?;

        let free_percent = if page_count > 0 {
            (freelist_count as f64 / page_count as f64) * 100.0
        } else {
            0.0
        };

        Ok(Self {
            page_count,
            freelist_count,
            page_size,
            free_percent,
            total_size_bytes: page_count * page_size,
            wasted_bytes: freelist_count * page_size,
        })
    }

    /// Returns `true` if `free_percent >= threshold_percent`.
    pub fn needs_vacuum(&self, threshold_percent: f64) -> bool {
        self.free_percent >= threshold_percent
    }

    async fn pragma_u64(conn: &libsql::Connection, name: &str) -> Result<u64> {
        let mut rows = conn
            .query(&format!("PRAGMA {name}"), ())
            .await
            .map_err(Error::from)?;
        let row = rows
            .next()
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::internal(format!("PRAGMA {name} returned no rows")))?;
        let val: i64 = row.get(0).map_err(Error::from)?;
        Ok(val as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_conn() -> libsql::Connection {
        let db = libsql::Builder::new_local(":memory:")
            .build()
            .await
            .unwrap();
        db.connect().unwrap()
    }

    #[tokio::test]
    async fn collect_returns_metrics_for_fresh_db() {
        let conn = test_conn().await;
        let health = DbHealth::collect(&conn).await.unwrap();

        assert!(health.page_count > 0);
        assert_eq!(health.freelist_count, 0);
        assert_eq!(health.page_size, 4096);
        assert_eq!(health.free_percent, 0.0);
        assert_eq!(health.total_size_bytes, health.page_count * 4096);
        assert_eq!(health.wasted_bytes, 0);
    }

    #[tokio::test]
    async fn needs_vacuum_threshold_logic() {
        let health = DbHealth {
            page_count: 100,
            freelist_count: 25,
            page_size: 4096,
            free_percent: 25.0,
            total_size_bytes: 100 * 4096,
            wasted_bytes: 25 * 4096,
        };

        assert!(health.needs_vacuum(20.0));
        assert!(health.needs_vacuum(25.0));
        assert!(!health.needs_vacuum(30.0));
    }
}
```

- [ ] **Step 2: Wire module in `src/db/mod.rs`**

Add after the `select` import at the bottom of `src/db/mod.rs` (before the `pub use libsql` line):

```rust
mod maintenance;
pub use maintenance::{DbHealth, run_vacuum, vacuum_handler, vacuum_if_needed, VacuumOptions, VacuumResult};
```

Note: `run_vacuum`, `VacuumOptions`, `VacuumResult`, `vacuum_handler` don't exist yet — the module will fail to compile until Task 2 and Task 3 add them. That's fine; we run tests scoped to what exists.

- [ ] **Step 3: Run the tests**

Run: `cargo test --features db db::maintenance --lib`

Expected: 2 tests pass (`collect_returns_metrics_for_fresh_db`, `needs_vacuum_threshold_logic`).

Note: The `pub use` line in `mod.rs` references symbols that don't exist yet. If the compiler blocks the test run, temporarily comment out the not-yet-existing re-exports (`run_vacuum`, `vacuum_handler`, `vacuum_if_needed`, `VacuumOptions`, `VacuumResult`) and only re-export `DbHealth`. Restore them in Task 2.

- [ ] **Step 4: Commit**

```bash
git add src/db/maintenance.rs src/db/mod.rs
git commit -m "feat(db): add DbHealth struct with collect and needs_vacuum"
```

---

### Task 2: `VacuumOptions`, `VacuumResult`, `run_vacuum`, `vacuum_if_needed`

**Files:**
- Modify: `src/db/maintenance.rs`
- Modify: `src/db/mod.rs` (restore re-exports if commented)

- [ ] **Step 1: Add types and functions with tests**

Add the following to `src/db/maintenance.rs`, above the `#[cfg(test)]` block:

```rust
/// Options for [`run_vacuum`].
pub struct VacuumOptions {
    /// Only vacuum if freelist exceeds this percentage. Default: `20.0`.
    pub threshold_percent: f64,
    /// Log-only mode — report health without running VACUUM. Default: `false`.
    pub dry_run: bool,
}

impl Default for VacuumOptions {
    fn default() -> Self {
        Self {
            threshold_percent: 20.0,
            dry_run: false,
        }
    }
}

/// Result of a [`run_vacuum`] call.
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

/// Run VACUUM with safety checks.
///
/// 1. Collects health metrics.
/// 2. If `free_percent < threshold` or `dry_run`, returns early.
/// 3. Executes `VACUUM`.
/// 4. Collects health metrics again.
///
/// Logs before/after metrics at `debug` level.
pub async fn run_vacuum(
    conn: &libsql::Connection,
    opts: VacuumOptions,
) -> Result<VacuumResult> {
    let start = std::time::Instant::now();
    let health_before = DbHealth::collect(conn).await?;

    tracing::debug!(
        page_count = health_before.page_count,
        freelist_count = health_before.freelist_count,
        free_pct = health_before.free_percent,
        wasted_bytes = health_before.wasted_bytes,
        "vacuum: health before"
    );

    if opts.dry_run || !health_before.needs_vacuum(opts.threshold_percent) {
        tracing::debug!(
            free_pct = health_before.free_percent,
            threshold = opts.threshold_percent,
            dry_run = opts.dry_run,
            "vacuum: skipped"
        );
        return Ok(VacuumResult {
            health_before,
            health_after: None,
            vacuumed: false,
            duration: start.elapsed(),
        });
    }

    conn.execute("VACUUM", ())
        .await
        .map_err(|e| Error::internal("VACUUM failed").chain(e))?;

    let health_after = DbHealth::collect(conn).await?;

    tracing::debug!(
        page_count = health_after.page_count,
        freelist_count = health_after.freelist_count,
        free_pct = health_after.free_percent,
        wasted_bytes = health_after.wasted_bytes,
        "vacuum: health after"
    );

    Ok(VacuumResult {
        health_before,
        health_after: Some(health_after),
        vacuumed: true,
        duration: start.elapsed(),
    })
}

/// Shorthand: run [`run_vacuum`] with the given threshold and default options.
pub async fn vacuum_if_needed(
    conn: &libsql::Connection,
    threshold_percent: f64,
) -> Result<VacuumResult> {
    run_vacuum(
        conn,
        VacuumOptions {
            threshold_percent,
            ..Default::default()
        },
    )
    .await
}
```

Add the following tests inside the `mod tests` block:

```rust
    #[tokio::test]
    async fn run_vacuum_skips_when_below_threshold() {
        let conn = test_conn().await;

        let result = run_vacuum(
            &conn,
            VacuumOptions {
                threshold_percent: 20.0,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert!(!result.vacuumed);
        assert!(result.health_after.is_none());
        assert_eq!(result.health_before.freelist_count, 0);
    }

    #[tokio::test]
    async fn run_vacuum_skips_in_dry_run() {
        let conn = test_conn().await;

        let result = run_vacuum(
            &conn,
            VacuumOptions {
                threshold_percent: 0.0, // would trigger on any freelist
                dry_run: true,
            },
        )
        .await
        .unwrap();

        assert!(!result.vacuumed);
        assert!(result.health_after.is_none());
    }

    #[tokio::test]
    async fn run_vacuum_executes_when_threshold_met() {
        let conn = test_conn().await;

        // Create a table, insert rows, delete them to produce freelist pages
        conn.execute(
            "CREATE TABLE bloat (id INTEGER PRIMARY KEY, data TEXT)",
            (),
        )
        .await
        .unwrap();

        // Insert enough data to create multiple pages
        for i in 0..500 {
            conn.execute(
                "INSERT INTO bloat (id, data) VALUES (?1, ?2)",
                libsql::params![i, "x".repeat(200)],
            )
            .await
            .unwrap();
        }

        // Delete all rows — pages go to freelist
        conn.execute("DELETE FROM bloat", ()).await.unwrap();

        let health = DbHealth::collect(&conn).await.unwrap();
        assert!(health.freelist_count > 0, "expected freelist pages after bulk delete");

        // Run vacuum with a very low threshold so it triggers
        let result = run_vacuum(
            &conn,
            VacuumOptions {
                threshold_percent: 0.0,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert!(result.vacuumed);
        let after = result.health_after.unwrap();
        assert!(
            after.freelist_count < health.freelist_count,
            "freelist should shrink after vacuum"
        );
    }

    #[tokio::test]
    async fn vacuum_if_needed_delegates_correctly() {
        let conn = test_conn().await;

        // Fresh DB, 0% free — should skip with threshold 20
        let result = vacuum_if_needed(&conn, 20.0).await.unwrap();
        assert!(!result.vacuumed);
    }
```

- [ ] **Step 2: Restore full re-exports in `src/db/mod.rs`**

If you commented out re-exports in Task 1 Step 3, restore them now. The full line should be:

```rust
pub use maintenance::{DbHealth, VacuumOptions, VacuumResult, run_vacuum, vacuum_handler, vacuum_if_needed};
```

Note: `vacuum_handler` still doesn't exist — if the compiler blocks, temporarily exclude it from the re-export. Restore it in Task 3.

- [ ] **Step 3: Run the tests**

Run: `cargo test --features db db::maintenance --lib`

Expected: 6 tests pass (2 from Task 1 + 4 new).

- [ ] **Step 4: Commit**

```bash
git add src/db/maintenance.rs src/db/mod.rs
git commit -m "feat(db): add run_vacuum and vacuum_if_needed with threshold guard"
```

---

### Task 3: `VacuumHandler` cron handler factory

**Files:**
- Modify: `src/db/maintenance.rs`
- Modify: `src/db/mod.rs` (restore `vacuum_handler` re-export if commented)

- [ ] **Step 1: Add `VacuumHandler` and `vacuum_handler` factory**

Add the following to `src/db/maintenance.rs`, after the `vacuum_if_needed` function and before the `#[cfg(test)]` block:

```rust
use crate::cron::context::{CronContext, FromCronContext};
use crate::cron::handler::CronHandler;
use crate::extractor::Service;

use super::Database;

/// Cron handler that checks database health and vacuums if the freelist
/// ratio exceeds the configured threshold. Logs results at `info` level.
///
/// Created by [`vacuum_handler`].
#[derive(Clone)]
pub struct VacuumHandler {
    threshold_percent: f64,
}

impl CronHandler<(Service<Database>,)> for VacuumHandler {
    async fn call(self, ctx: CronContext) -> Result<()> {
        let Service(db) = Service::<Database>::from_cron_context(&ctx)?;

        let result = run_vacuum(
            db.conn(),
            VacuumOptions {
                threshold_percent: self.threshold_percent,
                ..Default::default()
            },
        )
        .await?;

        if result.vacuumed {
            let after = result.health_after.as_ref().unwrap();
            tracing::info!(
                before_free_pct = result.health_before.free_percent,
                after_free_pct = after.free_percent,
                reclaimed_bytes = result.health_before.wasted_bytes - after.wasted_bytes,
                duration_ms = result.duration.as_millis(),
                "vacuum completed"
            );
        } else {
            tracing::info!(
                free_pct = result.health_before.free_percent,
                threshold = self.threshold_percent,
                "vacuum skipped, below threshold"
            );
        }

        Ok(())
    }
}

/// Returns a cron handler that checks DB health and vacuums if needed.
///
/// The handler extracts [`Service<Database>`] from the cron context.
/// Register the `Database` in the service registry before building the
/// scheduler.
///
/// # Example
///
/// ```rust,no_run
/// use modo::cron::Scheduler;
/// use modo::db::maintenance;
/// use modo::service::Registry;
///
/// # async fn example() -> modo::Result<()> {
/// let mut registry = Registry::new();
/// // registry.add(db.clone());
///
/// let scheduler = Scheduler::builder(&registry)
///     .job("0 3 * * 0", maintenance::vacuum_handler(20.0))?
///     .start()
///     .await;
/// # Ok(())
/// # }
/// ```
pub fn vacuum_handler(threshold_percent: f64) -> VacuumHandler {
    VacuumHandler { threshold_percent }
}
```

Note on imports: the `use` statements for cron types and `Service` should be placed at the top of the file with the other imports. The `CronHandler` trait uses RPITIT (`impl Future` in return position), so the manual impl above uses the same pattern — this compiles because `VacuumHandler` is a concrete type, not behind `dyn`.

- [ ] **Step 2: Restore `vacuum_handler` in `src/db/mod.rs` re-exports**

The full re-export line should now be:

```rust
pub use maintenance::{
    DbHealth, VacuumOptions, VacuumResult, run_vacuum, vacuum_handler, vacuum_if_needed,
};
```

- [ ] **Step 3: Run `cargo check --features db` to verify compilation**

Run: `cargo check --features db`

Expected: compiles with no errors. The `VacuumHandler` impl of `CronHandler` requires `Clone + Send + 'static` — all satisfied.

- [ ] **Step 4: Run all maintenance tests**

Run: `cargo test --features db db::maintenance --lib`

Expected: 6 tests pass (no new tests — `VacuumHandler` is tested indirectly through the existing `run_vacuum` tests and will be exercised end-to-end by the app developer).

- [ ] **Step 5: Commit**

```bash
git add src/db/maintenance.rs src/db/mod.rs
git commit -m "feat(db): add vacuum_handler cron handler factory"
```

---

### Task 4: Module docs and clippy

**Files:**
- Modify: `src/db/mod.rs` (doc comment table)
- Modify: `src/db/maintenance.rs` (if clippy issues)

- [ ] **Step 1: Update `src/db/mod.rs` doc comment**

Add a new `## Maintenance` section to the module doc comment in `src/db/mod.rs`. Insert it after the `## Configuration enums` table and before the `## Re-exports` section:

```rust
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
```

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --features db --tests -- -D warnings`

Expected: no warnings. Fix any clippy issues in `src/db/maintenance.rs` if they appear.

- [ ] **Step 3: Run fmt**

Run: `cargo fmt`

Expected: no changes (or auto-formats if needed).

- [ ] **Step 4: Run full test suite**

Run: `cargo test --features db`

Expected: all tests pass, including the 6 maintenance tests.

- [ ] **Step 5: Commit**

```bash
git add src/db/mod.rs src/db/maintenance.rs
git commit -m "docs(db): add maintenance section to module docs"
```
