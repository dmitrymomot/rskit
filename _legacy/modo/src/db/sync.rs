use crate::db::entity::EntityRegistration;
use crate::db::migration::MigrationRegistration;
use crate::error::Error;
use sea_orm::{ConnectionTrait, DatabaseConnection, Schema};
use tracing::info;

/// Synchronize database schema from registered entities, then run pending migrations.
///
/// Called during `AppBuilder::run()` after DB connection and WAL pragmas.
///
/// 1. Bootstrap `_modo_migrations` table (must exist before schema sync)
/// 2. Collect all `EntityRegistration` entries from `inventory`
/// 3. Register framework entities first, then user entities
/// 4. Run `SchemaBuilder::sync()` (addition-only, topo-sorted by SeaORM)
/// 5. Run extra SQL (composite indices, partial unique indices)
/// 6. Run pending migrations (version-ordered, tracked in `_modo_migrations`)
pub async fn sync_and_migrate(db: &DatabaseConnection) -> Result<(), Error> {
    // 1. Bootstrap _modo_migrations (raw SQL — must exist before sync
    //    so that migration tracking works even on first run)
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS _modo_migrations (\
            version INTEGER PRIMARY KEY, \
            description TEXT NOT NULL, \
            executed_at TEXT NOT NULL DEFAULT (datetime('now'))\
        )",
    )
    .await?;

    // 2. Collect and register all entities
    let backend = db.get_database_backend();
    let schema = Schema::new(backend);
    let mut builder = schema.builder();

    // Framework entities first, then user entities
    for reg in inventory::iter::<EntityRegistration> {
        if reg.is_framework {
            builder = (reg.register_fn)(builder);
        }
    }
    for reg in inventory::iter::<EntityRegistration> {
        if !reg.is_framework {
            builder = (reg.register_fn)(builder);
        }
    }

    // 3. Sync (addition-only — SeaORM handles topo sort)
    builder.sync(db).await?;
    info!("Schema sync complete");

    // 4. Run extra SQL (composite indices, partial unique indices, etc.)
    for reg in inventory::iter::<EntityRegistration> {
        for sql in reg.extra_sql {
            if let Err(e) = db.execute_unprepared(sql).await {
                tracing::error!(
                    table = reg.table_name,
                    sql = sql,
                    error = %e,
                    "Failed to execute extra SQL for entity"
                );
                return Err(e.into());
            }
        }
    }

    // 5. Run pending migrations
    run_pending_migrations(db).await?;

    Ok(())
}

async fn run_pending_migrations(db: &DatabaseConnection) -> Result<(), Error> {
    use crate::db::migration::migration_entity;
    use sea_orm::EntityTrait;
    use std::collections::HashSet;

    let mut migrations: Vec<&MigrationRegistration> = inventory::iter::<MigrationRegistration>
        .into_iter()
        .collect();

    if migrations.is_empty() {
        return Ok(());
    }

    // Check for duplicate versions
    let mut seen = HashSet::new();
    for m in &migrations {
        if !seen.insert(m.version) {
            return Err(Error::internal(format!(
                "Duplicate migration version: {}",
                m.version
            )));
        }
    }

    migrations.sort_by_key(|m| m.version);

    // Query already-executed versions
    let executed: Vec<migration_entity::Model> = migration_entity::Entity::find().all(db).await?;
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
            Error::internal(format!(
                "Migration version {} exceeds maximum ({})",
                migration.version,
                i64::MAX
            ))
        })?;
        let record = migration_entity::ActiveModel {
            version: sea_orm::Set(version_i64),
            description: sea_orm::Set(migration.description.to_string()),
            executed_at: sea_orm::Set(chrono::Utc::now().to_rfc3339()),
        };
        migration_entity::Entity::insert(record).exec(db).await?;
        info!("Migration v{} complete", migration.version);
    }

    Ok(())
}
