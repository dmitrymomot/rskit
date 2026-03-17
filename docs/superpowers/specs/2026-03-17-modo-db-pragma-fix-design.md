# modo-db: SQLite PRAGMA Fix + Configurable Settings

**Date:** 2026-03-17
**Status:** Approved
**Scope:** Bug fix + configuration enhancement (minor API change: `url` → `path` in `DatabaseConfig`)

## Problem

Two issues in `modo-db`'s SQLite connection setup:

1. **Per-connection PRAGMAs applied to only one connection.** `busy_timeout` and `foreign_keys` are per-connection settings. The current code runs them via `execute_unprepared` on a single connection from the pool (`connect.rs:33-48`). When sqlx lazily creates connections 2–N, those connections get none of these PRAGMAs — meaning some requests have `busy_timeout=0` (instant `SQLITE_BUSY`) and `foreign_keys=OFF`.

2. **PRAGMAs are not configurable.** All values are hardcoded. Users cannot tune `cache_size`, `mmap_size`, or `busy_timeout` without forking the crate.

## Approach

Use sqlx's `after_connect` hook to apply PRAGMAs on every new pool connection. Build the sqlx pool manually, then wrap it via `sea_orm::SqlxSqliteConnector::from_sqlx_sqlite_pool()` for SeaORM v2. This follows the [SeaORM cookbook pattern](https://www.sea-ql.org/sea-orm-cookbook/017-auto-execution-of-command-after-connection.html).

**Constraint:** SeaORM v2 only. Do not use any v1.x APIs or patterns.

**Verified:** `SqlxSqliteConnector::from_sqlx_sqlite_pool()` exists in SeaORM v2 ([docs.rs](https://docs.rs/sea-orm/latest/sea_orm/struct.SqlxSqliteConnector.html)). This implementation targets SeaORM v2 only — no v1.x compatibility.

Add a nested `SqliteConfig` struct for user-configurable PRAGMAs with sensible general-purpose defaults.

## Design

### Changes to `config.rs`

Add `SqliteConfig` nested inside `DatabaseConfig`. Use enums for string-valued PRAGMAs to prevent invalid values and config typos:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub max_connections: u32,       // 5 (unchanged)
    pub min_connections: u32,       // 1
    pub acquire_timeout_secs: u64,  // 30
    pub idle_timeout_secs: u64,     // 600
    pub max_lifetime_secs: u64,     // 1800
    pub sqlite: Option<SqliteDbConfig>,   // NEW — if set, use SQLite
    pub postgres: Option<PostgresDbConfig>, // NEW — if set, use Postgres
}

/// SQLite-specific config. Presence of this section selects SQLite backend.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SqliteDbConfig {
    pub path: String,               // "data/main.db"
    pub pragmas: SqliteConfig,      // PRAGMA tuning
}

/// Postgres-specific config. Presence of this section selects Postgres backend.
#[derive(Debug, Clone, Deserialize)]
pub struct PostgresDbConfig {
    pub url: String,                // "postgres://user:pass@host:5432/mydb?sslmode=require"
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum JournalMode {
    #[default]
    Wal,
    Delete,
    Truncate,
    Persist,
    Off,
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum SynchronousMode {
    Full,
    #[default]
    Normal,
    Off,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TempStore {
    Default,
    File,
    Memory,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SqliteConfig {
    pub journal_mode: JournalMode,       // WAL
    pub busy_timeout: u32,               // 5000
    pub synchronous: SynchronousMode,    // NORMAL
    pub foreign_keys: bool,              // true
    pub cache_size: i32,                 // -2000 (2MB, SQLite default)
    pub mmap_size: Option<i64>,          // None (opt-in)
    pub temp_store: Option<TempStore>,   // None (opt-in)
    pub wal_autocheckpoint: Option<u32>, // None (SQLite default 1000)
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            journal_mode: JournalMode::Wal,
            busy_timeout: 5000,
            synchronous: SynchronousMode::Normal,
            foreign_keys: true,
            cache_size: -2000,
            mmap_size: None,
            temp_store: None,
            wal_autocheckpoint: None,
        }
    }
}
```

All defaults match current behavior or SQLite defaults. `max_connections` stays at 5 — no behavioral changes beyond the bug fix. Users opt into performance tuning explicitly via YAML:

SQLite:
```yaml
database:
    sqlite:
        path: "data/app.db"
        pragmas:
            busy_timeout: 5000
            cache_size: -64000
```

Postgres:
```yaml
database:
    postgres:
        url: "postgres://user:pass@db.example.com:5432/myapp?sslmode=require"
```

Backend detection: `sqlite` and `postgres` are mutually exclusive. If both are set, error. If neither is set, defaults to SQLite with `path: "data/main.db"`.

Note: `journal_mode` is a database-level setting that persists across connections and restarts. Running it in `after_connect` is technically redundant but harmless — it ensures correctness if the database was previously opened with a different journal mode.

### Changes to `connect.rs`

Replace the current flow:

```
Current:  SeaORM ConnectOptions → Database::connect() → apply_sqlite_pragmas() on ONE connection
New:      config.sqlite is Some → SQLite path, config.postgres is Some → Postgres path
          SQLite:
            create parent dirs → build "sqlite://{path}?mode=rwc" URL
            → sqlx PoolOptions::new()
                .max_connections(...)
                .after_connect(|conn| { run PRAGMAs on THIS connection })
            → sqlx::SqlitePool
            → SqlxSqliteConnector::from_sqlx_sqlite_pool(pool)
            → DbPool
          Postgres path:
            → SeaORM ConnectOptions (unchanged)
            → Database::connect(opts)
            → DbPool
```

The `after_connect` closure captures `SqliteConfig` and applies PRAGMAs on every new connection. Enum types are rendered to their SQLite string values via `Display` impls:

```rust
async fn apply_sqlite_pragmas(
    conn: &mut sqlx::SqliteConnection,
    config: &SqliteConfig,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!("PRAGMA journal_mode={}", config.journal_mode))
        .execute(&mut *conn).await?;
    sqlx::query(&format!("PRAGMA busy_timeout={}", config.busy_timeout))
        .execute(&mut *conn).await?;
    sqlx::query(&format!("PRAGMA synchronous={}", config.synchronous))
        .execute(&mut *conn).await?;
    sqlx::query(&format!("PRAGMA foreign_keys={}", if config.foreign_keys { "ON" } else { "OFF" }))
        .execute(&mut *conn).await?;
    sqlx::query(&format!("PRAGMA cache_size={}", config.cache_size))
        .execute(&mut *conn).await?;
    if let Some(mmap) = config.mmap_size {
        sqlx::query(&format!("PRAGMA mmap_size={mmap}"))
            .execute(&mut *conn).await?;
    }
    if let Some(temp) = config.temp_store {
        sqlx::query(&format!("PRAGMA temp_store={temp}"))
            .execute(&mut *conn).await?;
    }
    if let Some(checkpoint) = config.wal_autocheckpoint {
        sqlx::query(&format!("PRAGMA wal_autocheckpoint={checkpoint}"))
            .execute(&mut *conn).await?;
    }
    Ok(())
}
```

For Postgres, the existing SeaORM `ConnectOptions` path remains unchanged.

### Changes to `Cargo.toml`

Add direct `sqlx` dependency (needed for `SqlitePoolOptions`, `SqliteConnectOptions`, and `SqlxSqliteConnector`):

```toml
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio-native-tls"], optional = true }
```

Gated behind the existing `sqlite` feature flag.

## Files Changed

| File                     | Change                                                                                                 |
| ------------------------ | ------------------------------------------------------------------------------------------------------ |
| `modo-db/src/config.rs`  | Replace `url` with `sqlite`/`postgres` sub-configs; add `SqliteConfig` struct with enums |
| `modo-db/src/connect.rs` | Build sqlx pool manually for SQLite with `after_connect`, keep SeaORM path for Postgres; resolve path → URL for SQLite |
| `modo-db/Cargo.toml`     | Add direct `sqlx` dependency behind `sqlite` feature                                                   |

3 files. API change: `DatabaseConfig.url` replaced with `sqlite`/`postgres` sub-configs. `connect()` signature, `DbPool`, and everything downstream remain identical.

Note: `modo-sqlite` (separate crate) will have its own independent PRAGMA configuration with the same enum types. The duplication is intentional — the crates have no dependency on each other.

## Testing

- Acquire multiple connections from the pool, query `PRAGMA busy_timeout` and `PRAGMA foreign_keys` on each — verify all return configured values (not SQLite defaults)
- Set custom `SqliteConfig` values via YAML, verify they are applied
- Set invalid enum variant in YAML — verify deserialization error
- Verify Postgres connection path is unaffected (no `after_connect` for Postgres)
- Verify default `SqliteConfig` produces same PRAGMA values as current hardcoded code
- Verify `after_connect` closure correctly captures cloned `SqliteConfig` (not a reference)
