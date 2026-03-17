# modo-db: SQLite PRAGMA Fix + Configurable Settings

**Date:** 2026-03-17
**Status:** Approved
**Scope:** Bug fix + configuration enhancement — no API changes

## Problem

Two issues in `modo-db`'s SQLite connection setup:

1. **Per-connection PRAGMAs applied to only one connection.** `busy_timeout` and `foreign_keys` are per-connection settings. The current code runs them via `execute_unprepared` on a single connection from the pool (`connect.rs:33-48`). When sqlx lazily creates connections 2–N, those connections get none of these PRAGMAs — meaning some requests have `busy_timeout=0` (instant `SQLITE_BUSY`) and `foreign_keys=OFF`.

2. **PRAGMAs are not configurable.** All values are hardcoded. Users cannot tune `cache_size`, `mmap_size`, or `busy_timeout` without forking the crate.

## Approach

Use sqlx's `after_connect` hook to apply PRAGMAs on every new pool connection. Build the sqlx pool manually, then wrap it via `SqlxSqliteConnector::from_sqlx_sqlite_pool()` for SeaORM. This follows the [SeaORM cookbook pattern](https://www.sea-ql.org/sea-orm-cookbook/017-auto-execution-of-command-after-connection.html).

Add a nested `SqliteConfig` struct for user-configurable PRAGMAs with sensible general-purpose defaults.

## Design

### Changes to `config.rs`

Add `SqliteConfig` nested inside `DatabaseConfig`:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,       // 10 (bumped from 5)
    pub min_connections: u32,       // 1
    pub acquire_timeout_secs: u64,  // 30
    pub idle_timeout_secs: u64,     // 600
    pub max_lifetime_secs: u64,     // 1800
    pub sqlite: SqliteConfig,       // NEW
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SqliteConfig {
    pub journal_mode: String,            // "WAL"
    pub busy_timeout: u32,               // 5000
    pub synchronous: String,             // "NORMAL"
    pub foreign_keys: bool,              // true
    pub cache_size: i32,                 // -2000 (2MB, SQLite default)
    pub mmap_size: Option<i64>,          // None (opt-in)
    pub temp_store: Option<String>,      // None (opt-in)
    pub wal_autocheckpoint: Option<u32>, // None (SQLite default 1000)
}
```

All defaults match current behavior or SQLite defaults. No surprise changes for existing code. Users opt into performance tuning explicitly via YAML:

```yaml
database:
  url: "sqlite://data.db?mode=rwc"
  max_connections: 10
  sqlite:
    busy_timeout: 5000
    cache_size: -64000
    mmap_size: 268435456
```

### Changes to `connect.rs`

Replace the current flow:

```
Current:  SeaORM ConnectOptions → Database::connect() → apply_sqlite_pragmas() on ONE connection
New:      sqlx PoolOptions::new()
            .max_connections(...)
            .min_connections(...)
            .acquire_timeout(...)
            .idle_timeout(...)
            .max_lifetime(...)
            .after_connect(|conn| { run PRAGMAs on THIS connection })
          → sqlx::SqlitePool
          → SqlxSqliteConnector::from_sqlx_sqlite_pool(pool)
          → wrap pool options from ConnectOptions for SeaORM compatibility
          → DbPool
```

The `after_connect` closure captures `SqliteConfig` and applies PRAGMAs on every new connection:

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
    if let Some(ref temp) = config.temp_store {
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

| File | Change |
|---|---|
| `modo-db/src/config.rs` | Add `SqliteConfig` struct, bump `max_connections` default to 10 |
| `modo-db/src/connect.rs` | Build sqlx pool manually for SQLite with `after_connect`, keep SeaORM path for Postgres |
| `modo-db/Cargo.toml` | Add direct `sqlx` dependency behind `sqlite` feature |

3 files. No API changes — `connect()` signature, `DbPool`, and everything downstream remain identical.

## Testing

- Verify PRAGMAs are applied on all pool connections (not just the first)
- Verify custom `SqliteConfig` values are respected
- Verify Postgres path is unaffected
- Verify default config produces same behavior as current code (except the bug fix)
