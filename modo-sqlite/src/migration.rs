use crate::pool::AsPool;

/// Registration entry for an embedded SQL migration.
///
/// Populated at compile time by `embed_migrations!` and collected via `inventory`.
/// You never construct this type manually.
pub struct MigrationRegistration {
    /// Numeric version derived from the `YYYYMMDDHHmmss` filename prefix.
    pub version: u64,
    /// Human-readable description derived from the filename suffix.
    pub description: &'static str,
    /// Migration group (defaults to `"default"`).
    pub group: &'static str,
    /// Full SQL content embedded at compile time via `include_str!`.
    pub sql: &'static str,
}

inventory::collect!(MigrationRegistration);

/// Run all pending migrations across every group.
///
/// Creates the `_modo_sqlite_migrations` tracking table if it does not yet exist.
/// Each migration is applied inside its own transaction and recorded atomically.
/// Already-executed migrations (by version number) are skipped, making this call
/// idempotent.
///
/// # Errors
///
/// Returns [`crate::Error`] if:
/// - Two registered migrations share the same version number (checked before any SQL runs).
/// - The tracking table cannot be created or queried.
/// - Any migration SQL fails to execute.
pub async fn run_migrations(pool: &impl AsPool) -> Result<(), crate::Error> {
    run_migrations_filtered(pool, |_| true).await
}

/// Run pending migrations for a single named group only.
///
/// All other groups are ignored. The global duplicate-version check still covers
/// all registered migrations (not just the selected group), so a version collision
/// in an excluded group is still detected and returns an error.
///
/// # Errors
///
/// Same conditions as [`run_migrations`].
pub async fn run_migrations_group(pool: &impl AsPool, group: &str) -> Result<(), crate::Error> {
    run_migrations_filtered(pool, |m| m.group == group).await
}

/// Run pending migrations for all groups except the ones listed in `exclude`.
///
/// Useful when multiple databases are in use and each database should only apply
/// its own group of migrations.
///
/// # Errors
///
/// Same conditions as [`run_migrations`].
pub async fn run_migrations_except(
    pool: &impl AsPool,
    exclude: &[&str],
) -> Result<(), crate::Error> {
    run_migrations_filtered(pool, |m| !exclude.contains(&m.group)).await
}

async fn run_migrations_filtered(
    pool: &impl AsPool,
    filter_fn: impl Fn(&MigrationRegistration) -> bool,
) -> Result<(), crate::Error> {
    // Create migrations table if not exists
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _modo_sqlite_migrations (
            version     BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            executed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(pool.pool())
    .await?;

    // Check for duplicate versions across ALL groups (versions must be globally unique)
    {
        let mut all: Vec<&MigrationRegistration> = inventory::iter::<MigrationRegistration>
            .into_iter()
            .collect();
        all.sort_by_key(|m| m.version);
        for window in all.windows(2) {
            if window[0].version == window[1].version {
                return Err(crate::Error::Query(sqlx::Error::Protocol(format!(
                    "duplicate migration version {} (groups '{}' and '{}')",
                    window[0].version, window[0].group, window[1].group
                ))));
            }
        }
    }

    // Collect from inventory, filtered by group
    let mut migrations: Vec<&MigrationRegistration> = inventory::iter::<MigrationRegistration>
        .into_iter()
        .filter(|m| filter_fn(m))
        .collect();

    // Sort by version
    migrations.sort_by_key(|m| m.version);

    // Query already-executed versions
    let executed: Vec<(i64,)> = sqlx::query_as("SELECT version FROM _modo_sqlite_migrations")
        .fetch_all(pool.pool())
        .await?;
    let executed_set: std::collections::HashSet<u64> =
        executed.iter().map(|r| r.0 as u64).collect();

    // Run pending migrations
    for m in migrations {
        if executed_set.contains(&m.version) {
            continue;
        }

        tracing::info!(
            version = m.version,
            description = m.description,
            group = m.group,
            "running migration"
        );

        // Execute the SQL and record it in a transaction
        let mut tx = pool.pool().begin().await?;
        sqlx::query(m.sql).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO _modo_sqlite_migrations (version, description) VALUES (?, ?)")
            .bind(m.version as i64)
            .bind(m.description)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }

    Ok(())
}
