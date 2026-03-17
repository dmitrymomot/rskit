# modo-db PRAGMA Fix Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix per-connection PRAGMA bug and add configurable SQLite settings to modo-db.

**Architecture:** Build sqlx pool manually with `after_connect` hook for SQLite, wrap via `SqlxSqliteConnector::from_sqlx_sqlite_pool()` for SeaORM v2. Add `SqliteDbConfig`/`SqliteConfig` structs with enum-typed PRAGMAs. Postgres path unchanged.

**Tech Stack:** Rust, SeaORM v2, sqlx 0.8, serde, thiserror

**Spec:** `docs/superpowers/specs/2026-03-17-modo-db-pragma-fix-design.md`

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `modo-db/src/config.rs` | Modify | Replace `url: String` with `sqlite`/`postgres` sub-configs; add `SqliteConfig`, `SqliteDbConfig`, `PostgresDbConfig`, PRAGMA enums |
| `modo-db/src/connect.rs` | Modify | Build sqlx pool with `after_connect` for SQLite; keep SeaORM `ConnectOptions` for Postgres |
| `modo-db/Cargo.toml` | Modify | Add direct `sqlx` dep behind `sqlite` feature |
| `modo-db/tests/config.rs` | Modify | Update tests for new config shape |
| `modo-db/tests/connect.rs` | Modify | Update tests to use new config; add PRAGMA verification tests |

---

### Task 1: Add PRAGMA enums and SqliteConfig to config.rs

**Files:**
- Modify: `modo-db/src/config.rs`
- Modify: `modo-db/tests/config.rs`

- [ ] **Step 1: Write failing test for new config deserialization**

```rust
// modo-db/tests/config.rs — add this test
#[test]
fn sqlite_sub_config_deserialization() {
    let yaml = r#"
sqlite:
    path: "data/test.db"
    pragmas:
        busy_timeout: 3000
        cache_size: -8000
        journal_mode: DELETE
        synchronous: FULL
"#;
    let config: DatabaseConfig = serde_yaml_ng::from_str(yaml).unwrap();
    let sqlite = config.sqlite.unwrap();
    assert_eq!(sqlite.path, "data/test.db");
    assert_eq!(sqlite.pragmas.busy_timeout, 3000);
    assert_eq!(sqlite.pragmas.cache_size, -8000);
    assert!(matches!(sqlite.pragmas.journal_mode, JournalMode::Delete));
    assert!(matches!(sqlite.pragmas.synchronous, SynchronousMode::Full));
}

#[test]
fn postgres_sub_config_deserialization() {
    let yaml = r#"
postgres:
    url: "postgres://localhost/test"
"#;
    let config: DatabaseConfig = serde_yaml_ng::from_str(yaml).unwrap();
    let pg = config.postgres.unwrap();
    assert_eq!(pg.url, "postgres://localhost/test");
    assert!(config.sqlite.is_none());
}

#[test]
fn default_config_has_sqlite() {
    let config = DatabaseConfig::default();
    assert!(config.sqlite.is_some());
    assert!(config.postgres.is_none());
    let sqlite = config.sqlite.unwrap();
    assert_eq!(sqlite.path, "data/main.db");
    assert_eq!(sqlite.pragmas.busy_timeout, 5000);
    assert!(matches!(sqlite.pragmas.journal_mode, JournalMode::Wal));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo-db config -- --nocapture`
Expected: FAIL — `SqliteDbConfig`, `PostgresDbConfig`, `JournalMode` types don't exist yet.

- [ ] **Step 3: Implement config structs**

Replace `DatabaseConfig` in `modo-db/src/config.rs` with the full config from the spec: `DatabaseConfig` with `sqlite: Option<SqliteDbConfig>`, `postgres: Option<PostgresDbConfig>`, `SqliteDbConfig`, `PostgresDbConfig`, `SqliteConfig`, `JournalMode`, `SynchronousMode`, `TempStore` enums, `Default` impls, and `Display` impls for enums (for PRAGMA formatting).

`Display` impls must map enum variants to SQLite PRAGMA string values:
- `JournalMode`: `Wal` → `"WAL"`, `Delete` → `"DELETE"`, `Truncate` → `"TRUNCATE"`, `Persist` → `"PERSIST"`, `Off` → `"OFF"`
- `SynchronousMode`: `Full` → `"FULL"`, `Normal` → `"NORMAL"`, `Off` → `"OFF"`
- `TempStore`: `Default` → `"DEFAULT"`, `File` → `"FILE"`, `Memory` → `"MEMORY"`

Default for `DatabaseConfig` should set `sqlite: Some(SqliteDbConfig::default())` and `postgres: None`.

**Important:** `config.rs` has inline `#[cfg(test)] mod tests` at lines 37-64 with `default_timeout_values` and `partial_yaml_deserialization`. These inline tests must also be updated to match the new config shape, in addition to the external `tests/config.rs` file.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p modo-db config -- --nocapture`
Expected: All config tests PASS.

- [ ] **Step 5: Commit**

```bash
git add modo-db/src/config.rs modo-db/tests/config.rs
git commit -m "feat(modo-db): add SqliteConfig with PRAGMA enums and sqlite/postgres sub-configs"
```

---

### Task 2: Add direct sqlx dependency

**Files:**
- Modify: `modo-db/Cargo.toml`

- [ ] **Step 1: Add sqlx dependency behind sqlite feature**

In `modo-db/Cargo.toml`, add to `[dependencies]`:

```toml
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio-native-tls"], optional = true }
```

Update the `sqlite` feature to include it:

```toml
[features]
default = ["sqlite"]
sqlite = ["sea-orm/sqlx-sqlite", "dep:sqlx"]
postgres = ["sea-orm/sqlx-postgres"]
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p modo-db`
Expected: Compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add modo-db/Cargo.toml
git commit -m "feat(modo-db): add direct sqlx dependency behind sqlite feature"
```

---

### Task 3: Rewrite connect.rs for per-connection PRAGMAs

**Files:**
- Modify: `modo-db/src/connect.rs`
- Modify: `modo-db/tests/connect.rs`

- [ ] **Step 1: Write failing test for per-connection PRAGMAs**

```rust
// modo-db/tests/connect.rs — replace existing tests with new config shape
use modo_db::config::{DatabaseConfig, SqliteDbConfig, SqliteConfig};

#[tokio::test]
async fn test_connect_sqlite_in_memory() {
    let config = DatabaseConfig {
        sqlite: Some(SqliteDbConfig {
            path: ":memory:".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let db = modo_db::connect(&config).await.unwrap();
    use sea_orm::ConnectionTrait;
    db.execute_unprepared("SELECT 1").await.unwrap();
}

#[tokio::test]
async fn test_pragmas_applied_on_all_connections() {
    let config = DatabaseConfig {
        max_connections: 3,
        min_connections: 3,
        sqlite: Some(SqliteDbConfig {
            path: ":memory:".to_string(),
            pragmas: SqliteConfig {
                busy_timeout: 7777,
                ..Default::default()
            },
            ..Default::default()
        }),
        ..Default::default()
    };
    let db = modo_db::connect(&config).await.unwrap();

    // Query PRAGMA on multiple connections by running concurrent queries.
    // Each should return the configured value, not the default.
    use sea_orm::ConnectionTrait;
    for _ in 0..3 {
        let result = db
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "PRAGMA busy_timeout".to_string(),
            ))
            .await
            .unwrap()
            .unwrap();
        use sea_orm::TryGetable;
        let timeout: i32 = result.try_get_by_index(0).unwrap();
        assert_eq!(timeout, 7777);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p modo-db connect -- --nocapture`
Expected: FAIL — the current `connect()` doesn't accept new config shape, and PRAGMAs are only applied to one connection.

- [ ] **Step 3: Implement new connect() with after_connect**

Rewrite `modo-db/src/connect.rs`:
- Detect backend from `config.sqlite` / `config.postgres` (mutually exclusive, error if both set)
- SQLite path: build sqlx pool manually with `SqlitePoolOptions`, set `after_connect` to call `apply_sqlite_pragmas()`, then wrap with `SqlxSqliteConnector::from_sqlx_sqlite_pool()`
- Postgres path: use SeaORM `ConnectOptions::new(&pg.url)` as before, apply pool settings
- SQLite path resolution: create parent dirs if needed, build `sqlite://{path}?mode=rwc` URL
- Keep `redact_url()` helper

The `apply_sqlite_pragmas` function takes `&mut sqlx::SqliteConnection` and `&SqliteConfig`, runs PRAGMAs using `format!` with `Display` impls on enums.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p modo-db connect -- --nocapture`
Expected: All tests PASS, including the PRAGMA verification test.

- [ ] **Step 5: Run full test suite**

Run: `cargo test -p modo-db`
Expected: All tests PASS.

- [ ] **Step 6: Run sync tests to verify they still work**

Run: `cargo test -p modo-db sync -- --nocapture`
Expected: `test_sync_and_migrate_empty` and `test_sync_and_migrate_group` PASS with new config.

Note: these tests reference `url: "sqlite::memory:"` — they need updating to use the new `sqlite: Some(SqliteDbConfig { path: ":memory:" })` config shape.

- [ ] **Step 7: Commit**

```bash
git add modo-db/src/connect.rs modo-db/tests/connect.rs
git commit -m "fix(modo-db): apply SQLite PRAGMAs per-connection via after_connect hook"
```

---

### Task 4a: Update example YAML configs and Rust configs

**Files:**
- Modify: `examples/todo-api/config/development.yaml` — change `database.url` to `database.sqlite.path`
- Modify: `examples/sse-chat/config/development.yaml` — same YAML transform

- [ ] **Step 1: Update YAML files**

In each YAML, replace:
```yaml
database:
  url: "${DATABASE_URL:-sqlite://...}"
```
with:
```yaml
database:
  sqlite:
    path: "${DATABASE_URL:-data/app.db}"
```

- [ ] **Step 2: Verify examples compile**

Run: `cargo check -p todo-api && cargo check -p sse-chat`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add examples/todo-api/config/development.yaml examples/sse-chat/config/development.yaml
git commit -m "refactor: update example YAML configs for new DatabaseConfig shape"
```

---

### Task 4b: Update test files that construct DatabaseConfig

**Files:**
- Modify: `modo-session/tests/common/mod.rs` — uses `DatabaseConfig { url: "sqlite::memory:" }`
- Modify: `modo-auth/tests/integration.rs` — same pattern

- [ ] **Step 1: Update test configs**

Replace `DatabaseConfig { url: "sqlite::memory:".to_string(), ..Default::default() }` with:
```rust
DatabaseConfig {
    sqlite: Some(SqliteDbConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    }),
    ..Default::default()
}
```

Add appropriate `use` imports for `SqliteDbConfig`.

- [ ] **Step 2: Verify tests pass**

Run: `cargo test -p modo-session && cargo test -p modo-auth`
Expected: All PASS.

- [ ] **Step 3: Commit**

```bash
git add modo-session/tests/common/mod.rs modo-auth/tests/integration.rs
git commit -m "refactor: update modo-session and modo-auth tests for new config shape"
```

---

### Task 4c: Update CLI templates

**Files:**
- Modify: `modo-cli/templates/web/config/development.yaml.jinja`
- Modify: `modo-cli/templates/web/config/production.yaml.jinja`
- Modify: `modo-cli/templates/api/config/development.yaml.jinja`
- Modify: `modo-cli/templates/api/config/production.yaml.jinja`
- Modify: `modo-cli/templates/worker/config/development.yaml.jinja`
- Modify: `modo-cli/templates/worker/config/production.yaml.jinja`
- Modify: `modo-cli/templates/web/.env.jinja` and `.env.example.jinja`
- Modify: `modo-cli/templates/worker/.env.jinja` and `.env.example.jinja`
- Modify: `modo-cli/templates/api/.env.jinja` and `.env.example.jinja`

- [ ] **Step 1: Update all YAML templates**

Change `database.url` references to `database.sqlite.path` in all YAML jinja templates. Keep `${DATABASE_URL}` env var interpolation but map it to the path field.

- [ ] **Step 2: Update Rust config templates if needed**

Check `modo-cli/templates/*/src/config.rs.jinja` — these use `DatabaseConfig` by name, so they should still compile. Verify no `url` field access.

- [ ] **Step 3: Verify CLI templates render correctly**

Run: `cargo check -p modo-cli`
Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
git add modo-cli/templates/
git commit -m "refactor: update CLI scaffold templates for new DatabaseConfig shape"
```

---

### Task 4d: Update documentation references

**Files:**
- Modify: `modo-db/README.md` — update YAML examples and config descriptions
- Modify: `claude-plugin/skills/modo/references/database.md` — update config examples
- Modify: `claude-plugin/skills/modo/references/config.md` — update config structure

- [ ] **Step 1: Update all documentation**

Change all `url: "sqlite://..."` references to the new `sqlite: path:` sub-config structure.

- [ ] **Step 2: Commit**

```bash
git add modo-db/README.md claude-plugin/skills/modo/references/
git commit -m "docs: update database config documentation for new sqlite/postgres sub-configs"
```

---

### Task 5: Lint, format, and full workspace verification

- [ ] **Step 1: Run formatter and linter**

Run: `just fmt && just lint`
Expected: No errors or warnings.

- [ ] **Step 2: Run full workspace tests**

Run: `just test`
Expected: All workspace tests PASS.

- [ ] **Step 3: Fix any issues found**

- [ ] **Step 4: Commit if changes needed**

Stage only changed files explicitly (not `git add -A`):

```bash
git add <changed-files>
git commit -m "chore: fix formatting and lint warnings"
```
