use crate::pool::AsPool;

/// Registration entry for an embedded SQL migration.
/// Collected via `inventory` from `embed_migrations!()` calls.
pub struct MigrationRegistration {
    pub version: u64,
    pub description: &'static str,
    pub group: &'static str,
    pub sql: &'static str,
}

inventory::collect!(MigrationRegistration);

/// Run ALL pending migrations (every group).
pub async fn run_migrations(pool: &impl AsPool) -> Result<(), crate::Error> {
    run_migrations_filtered(pool, |_| true).await
}

/// Run pending migrations for a specific group only.
pub async fn run_migrations_group(pool: &impl AsPool, group: &str) -> Result<(), crate::Error> {
    run_migrations_filtered(pool, |m| m.group == group).await
}

/// Run pending migrations for all groups except the excluded ones.
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

    // Collect from inventory
    let mut migrations: Vec<&MigrationRegistration> = inventory::iter::<MigrationRegistration>
        .into_iter()
        .filter(|m| filter_fn(m))
        .collect();

    // Sort by version
    migrations.sort_by_key(|m| m.version);

    // Check for duplicate versions
    for window in migrations.windows(2) {
        if window[0].version == window[1].version {
            return Err(crate::Error::Query(sqlx::Error::Protocol(format!(
                "duplicate migration version: {}",
                window[0].version
            ))));
        }
    }

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

        // Execute the SQL
        sqlx::query(m.sql).execute(pool.pool()).await?;

        // Record it
        sqlx::query("INSERT INTO _modo_sqlite_migrations (version, description) VALUES (?, ?)")
            .bind(m.version as i64)
            .bind(m.description)
            .execute(pool.pool())
            .await?;
    }

    Ok(())
}
