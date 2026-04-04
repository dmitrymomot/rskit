use crate::cron::{CronContext, CronHandler, FromCronContext};
use crate::error::{Error, Result};
use crate::extractor::Service;

use super::Database;

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
    ///
    /// # Errors
    ///
    /// Returns an error if any PRAGMA query fails or returns an unexpected value.
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
        u64::try_from(val)
            .map_err(|_| Error::internal(format!("PRAGMA {name} returned negative value: {val}")))
    }
}

/// Options for [`run_vacuum`].
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
///
/// # Errors
///
/// Returns an error if health collection or the `VACUUM` statement fails.
pub async fn run_vacuum(conn: &libsql::Connection, opts: VacuumOptions) -> Result<VacuumResult> {
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
///
/// # Errors
///
/// Returns an error if health collection or the `VACUUM` statement fails.
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
                reclaimed_bytes = result
                    .health_before
                    .wasted_bytes
                    .saturating_sub(after.wasted_bytes),
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
/// use modo::db;
/// use modo::service::Registry;
///
/// # async fn example() -> modo::Result<()> {
/// let mut registry = Registry::new();
/// // registry.add(db.clone());
///
/// let scheduler = Scheduler::builder(&registry)
///     .job("0 3 * * 0", db::vacuum_handler(20.0))?
///     .start()
///     .await;
/// # Ok(())
/// # }
/// ```
pub fn vacuum_handler(threshold_percent: f64) -> VacuumHandler {
    VacuumHandler { threshold_percent }
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
        // Create a table to force page allocation in the in-memory database.
        conn.execute("CREATE TABLE _health_probe (id INTEGER PRIMARY KEY)", ())
            .await
            .unwrap();
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
        conn.execute("CREATE TABLE bloat (id INTEGER PRIMARY KEY, data TEXT)", ())
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
        assert!(
            health.freelist_count > 0,
            "expected freelist pages after bulk delete"
        );

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
}
