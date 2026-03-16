use crate::entity::EntityRegistration;
use crate::migration::MigrationRegistration;
use crate::pool::DbPool;
use sea_orm::{ConnectionTrait, Schema};
use tracing::info;

/// Synchronize database schema from all registered entities, then run all pending migrations.
///
/// 1. Bootstrap `_modo_migrations` table (must exist before schema sync)
/// 2. Collect all `EntityRegistration` entries from `inventory`
/// 3. Register framework entities first, then user entities
/// 4. Run `SchemaBuilder::sync()` (addition-only, topo-sorted by SeaORM)
/// 5. Execute extra SQL (composite indices, partial unique indices)
/// 6. Run pending migrations (version-ordered, tracked in `_modo_migrations`)
pub async fn sync_and_migrate(db: &DbPool) -> Result<(), modo::Error> {
    do_sync(db, None).await
}

/// Synchronize database schema for entities and migrations belonging to the named group only.
///
/// This is useful when entities in a specific group live in a separate database (e.g.
/// SQLite jobs database). The `_modo_migrations` table is always bootstrapped in the
/// target database.
pub async fn sync_and_migrate_group(db: &DbPool, group: &str) -> Result<(), modo::Error> {
    do_sync(db, Some(group)).await
}

async fn do_sync(db: &DbPool, group_filter: Option<&str>) -> Result<(), modo::Error> {
    let conn = db.connection();

    // 1. Bootstrap _modo_migrations (BIGINT + CURRENT_TIMESTAMP work on both SQLite and Postgres)
    conn.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS _modo_migrations (\
            version BIGINT PRIMARY KEY, \
            description TEXT NOT NULL, \
            executed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP\
        )",
    )
    .await
    .map_err(|e| modo::Error::internal(format!("failed to bootstrap migrations table: {e}")))?;

    // 2. Collect entities, optionally filtered by group
    let (framework, user): (Vec<_>, Vec<_>) = inventory::iter::<EntityRegistration>
        .into_iter()
        .filter(|r| match group_filter {
            Some(g) => r.group == g,
            None => true,
        })
        .partition(|r| r.is_framework);

    let backend = conn.get_database_backend();
    let schema = Schema::new(backend);
    let mut builder = schema.builder();

    // Register framework entities first, then user entities
    for reg in &framework {
        builder = (reg.register_fn)(builder);
    }
    for reg in &user {
        builder = (reg.register_fn)(builder);
    }

    // 3. Sync (addition-only — SeaORM handles topo sort)
    builder
        .sync(conn)
        .await
        .map_err(|e| modo::Error::internal(format!("schema sync failed: {e}")))?;
    info!("Schema sync complete");

    // 4. Run extra SQL (composite indices, partial unique indices, etc.)
    for reg in framework.iter().chain(user.iter()) {
        for sql in reg.extra_sql {
            if let Err(e) = conn.execute_unprepared(sql).await {
                tracing::error!(
                    table = reg.table_name,
                    sql = sql,
                    error = %e,
                    "Failed to execute extra SQL for entity"
                );
                return Err(modo::Error::internal(format!(
                    "extra SQL for {} failed: {e}",
                    reg.table_name
                )));
            }
        }
    }

    // 5. Run pending migrations
    run_pending_migrations(conn, group_filter).await?;

    Ok(())
}

async fn run_pending_migrations(
    db: &sea_orm::DatabaseConnection,
    group_filter: Option<&str>,
) -> Result<(), modo::Error> {
    use crate::migration::migration_entity;
    use sea_orm::EntityTrait;
    use std::collections::HashSet;

    let mut migrations: Vec<&MigrationRegistration> = inventory::iter::<MigrationRegistration>
        .into_iter()
        .filter(|m| match group_filter {
            Some(g) => m.group == g,
            None => true,
        })
        .collect();

    if migrations.is_empty() {
        return Ok(());
    }

    // Check for duplicate versions
    let mut seen = HashSet::new();
    for m in &migrations {
        if !seen.insert(m.version) {
            return Err(modo::Error::internal(format!(
                "duplicate migration version: {}",
                m.version
            )));
        }
    }

    migrations.sort_by_key(|m| m.version);

    // Query already-executed versions
    let executed: Vec<migration_entity::Model> = migration_entity::Entity::find()
        .all(db)
        .await
        .map_err(|e| modo::Error::internal(format!("failed to query migrations: {e}")))?;
    let executed_versions: HashSet<u64> = executed.iter().map(|m| m.version as u64).collect();

    // Run pending
    for migration in &migrations {
        if executed_versions.contains(&migration.version) {
            continue;
        }
        info!(
            "Running migration v{}: {}",
            migration.version, migration.description
        );

        (migration.handler)(db).await?;

        // Record migration as executed
        let version_i64 = i64::try_from(migration.version).map_err(|_| {
            modo::Error::internal(format!(
                "migration version {} exceeds maximum ({})",
                migration.version,
                i64::MAX
            ))
        })?;
        let record = migration_entity::ActiveModel {
            version: sea_orm::Set(version_i64),
            description: sea_orm::Set(migration.description.to_string()),
            executed_at: sea_orm::Set(chrono::Utc::now().to_rfc3339()),
        };
        migration_entity::Entity::insert(record)
            .exec(db)
            .await
            .map_err(|e| modo::Error::internal(format!("failed to record migration: {e}")))?;
        info!("Migration v{} complete", migration.version);
    }

    Ok(())
}
