# SQLite Driver Benchmark — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Benchmark sqlx vs rusqlite vs libsql across writes, single-connection reads, and pooled reads on two schemas with ops/sec and peak RSS.

**Architecture:** Single `main.rs` binary that runs 18 benchmarks sequentially (3 drivers × 2 schemas × 3 scenarios). Each driver module is a set of free functions. Memory measured via macOS `mach_task_info`. Results printed as a formatted comparison table.

**Tech Stack:** tokio, sqlx 0.8 (sqlite), rusqlite 0.39 (bundled), r2d2 0.8 + r2d2_sqlite 0.33, libsql 0.9, rand 0.10

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `experiments/sqlite-bench/Cargo.toml` | Create | Dependencies |
| `experiments/sqlite-bench/src/main.rs` | Create | All benchmark logic, output formatting |

---

### Task 1: Scaffold project and verify dependencies

**Files:**
- Create: `experiments/sqlite-bench/Cargo.toml`
- Create: `experiments/sqlite-bench/src/main.rs`

- [ ] **Step 1: Create Cargo.toml**

Create `experiments/sqlite-bench/Cargo.toml`:

```toml
[package]
name = "sqlite-bench"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"], default-features = false }
rusqlite = { version = "0.39", features = ["bundled"] }
r2d2 = "0.8"
r2d2_sqlite = "0.33"
libsql = "0.9"
rand = "0.10"
libc = "0.2"
```

- [ ] **Step 2: Create minimal main.rs**

Create `experiments/sqlite-bench/src/main.rs`:

```rust
fn main() {
    println!("sqlite-bench placeholder");
}
```

- [ ] **Step 3: Verify dependencies resolve**

```bash
cd experiments/sqlite-bench && cargo check
```

Expected: Downloads all crates, compiles, no errors.

---

### Task 2: Implement memory measurement and benchmark harness

**Files:**
- Modify: `experiments/sqlite-bench/src/main.rs`

- [ ] **Step 1: Write the complete main.rs**

Replace `experiments/sqlite-bench/src/main.rs` with the full benchmark implementation:

```rust
use std::sync::Arc;
use std::time::Instant;

use rand::seq::SliceRandom;

const ITERATIONS: usize = 100_000;
const POOL_SIZE: usize = 10;
const TASKS: usize = 10;

// ─── Memory measurement (macOS) ────────────────────────────────────

fn rss_mb() -> f64 {
    use std::mem::MaybeUninit;
    unsafe {
        let mut info = MaybeUninit::<libc::mach_task_basic_info_data_t>::uninit();
        let mut count = (std::mem::size_of::<libc::mach_task_basic_info_data_t>()
            / std::mem::size_of::<libc::natural_t>()) as libc::mach_msg_type_number_t;
        let kr = libc::task_info(
            libc::mach_task_self(),
            libc::MACH_TASK_BASIC_INFO,
            info.as_mut_ptr() as libc::task_info_t,
            &mut count,
        );
        if kr != libc::KERN_SUCCESS {
            return 0.0;
        }
        info.assume_init().resident_size as f64 / (1024.0 * 1024.0)
    }
}

// ─── Result types ──────────────────────────────────────────────────

struct BenchResult {
    writes_per_sec: f64,
    reads_single_per_sec: f64,
    reads_pool_per_sec: f64,
    peak_rss_mb: f64,
}

fn ops_per_sec(iterations: usize, elapsed: std::time::Duration) -> f64 {
    iterations as f64 / elapsed.as_secs_f64()
}

// ─── Data generation ───────────────────────────────────────────────

fn kv_insert_data(i: usize) -> (String, String) {
    (format!("key_{i}"), format!("value_{i}"))
}

fn user_insert_data(i: usize) -> (String, String, String, String, String) {
    (
        format!("user_{i}"),
        format!("User Name {i}"),
        format!("user{i}@example.com"),
        format!("This is a bio for user {i}. It contains some text to make the row wider and more realistic for benchmarking purposes."),
        format!("2026-01-{:02}T12:00:00Z", (i % 28) + 1),
    )
}

fn random_keys(prefix: &str, count: usize) -> Vec<String> {
    let mut keys: Vec<String> = (0..count).map(|i| format!("{prefix}_{i}")).collect();
    let mut rng = rand::rng();
    keys.shuffle(&mut rng);
    keys
}

// ─── Schema enum ───────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum Schema {
    Kv,
    Users,
}

impl Schema {
    fn name(self) -> &'static str {
        match self {
            Schema::Kv => "kv (key TEXT, value TEXT)",
            Schema::Users => "users (id, name, email, bio, created_at)",
        }
    }

    fn create_sql(self) -> &'static str {
        match self {
            Schema::Kv => "CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT)",
            Schema::Users => "CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, name TEXT NOT NULL, email TEXT NOT NULL, bio TEXT NOT NULL, created_at TEXT NOT NULL)",
        }
    }

    fn insert_sql(self) -> &'static str {
        match self {
            Schema::Kv => "INSERT INTO kv (key, value) VALUES (?1, ?2)",
            Schema::Users => "INSERT INTO users (id, name, email, bio, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        }
    }

    fn select_sql(self) -> &'static str {
        match self {
            Schema::Kv => "SELECT key, value FROM kv WHERE key = ?1",
            Schema::Users => "SELECT id, name, email, bio, created_at FROM users WHERE id = ?1",
        }
    }

    fn key_prefix(self) -> &'static str {
        match self {
            Schema::Kv => "key",
            Schema::Users => "user",
        }
    }
}

// ─── sqlx benchmarks ───────────────────────────────────────────────

mod bench_sqlx {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteJournalMode, SqliteSynchronous};
    use sqlx::{ConnectOptions, Row};
    use std::str::FromStr;

    async fn create_pool(db_path: &str, size: u32) -> sqlx::SqlitePool {
        let opts = SqliteConnectOptions::from_str(db_path)
            .unwrap()
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(false);
        SqlitePoolOptions::new()
            .max_connections(size)
            .connect_with(opts)
            .await
            .unwrap()
    }

    pub async fn run(schema: Schema) -> BenchResult {
        let db_path = "bench_sqlx.db";
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{db_path}-wal"));
        let _ = std::fs::remove_file(format!("{db_path}-shm"));

        let mut peak_rss = rss_mb();

        // ── Writes ──
        let pool = create_pool(db_path, 1).await;
        sqlx::query(schema.create_sql()).execute(&pool).await.unwrap();

        let start = Instant::now();
        for i in 0..ITERATIONS {
            match schema {
                Schema::Kv => {
                    let (k, v) = kv_insert_data(i);
                    sqlx::query(schema.insert_sql())
                        .bind(&k).bind(&v)
                        .execute(&pool).await.unwrap();
                }
                Schema::Users => {
                    let (id, name, email, bio, ts) = user_insert_data(i);
                    sqlx::query(schema.insert_sql())
                        .bind(&id).bind(&name).bind(&email).bind(&bio).bind(&ts)
                        .execute(&pool).await.unwrap();
                }
            }
        }
        let writes_per_sec = ops_per_sec(ITERATIONS, start.elapsed());
        peak_rss = peak_rss.max(rss_mb());
        pool.close().await;

        // ── Reads (single connection) ──
        let pool = create_pool(db_path, 1).await;
        let keys = random_keys(schema.key_prefix(), ITERATIONS);

        let start = Instant::now();
        for key in &keys {
            let _row = sqlx::query(schema.select_sql())
                .bind(key)
                .fetch_one(&pool).await.unwrap();
        }
        let reads_single_per_sec = ops_per_sec(ITERATIONS, start.elapsed());
        peak_rss = peak_rss.max(rss_mb());
        pool.close().await;

        // ── Reads (pool) ──
        let pool = create_pool(db_path, POOL_SIZE as u32).await;
        let keys = Arc::new(random_keys(schema.key_prefix(), ITERATIONS));
        let chunk_size = ITERATIONS / TASKS;

        let start = Instant::now();
        let mut handles = Vec::new();
        for t in 0..TASKS {
            let pool = pool.clone();
            let keys = keys.clone();
            let sql = schema.select_sql();
            let offset = t * chunk_size;
            handles.push(tokio::spawn(async move {
                for i in offset..offset + chunk_size {
                    let _row = sqlx::query(sql)
                        .bind(&keys[i])
                        .fetch_one(&pool).await.unwrap();
                }
            }));
        }
        for h in handles { h.await.unwrap(); }
        let reads_pool_per_sec = ops_per_sec(ITERATIONS, start.elapsed());
        peak_rss = peak_rss.max(rss_mb());
        pool.close().await;

        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{db_path}-wal"));
        let _ = std::fs::remove_file(format!("{db_path}-shm"));

        BenchResult { writes_per_sec, reads_single_per_sec, reads_pool_per_sec, peak_rss_mb: peak_rss }
    }
}

// ─── rusqlite benchmarks ───────────────────────────────────────────

mod bench_rusqlite {
    use super::*;
    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;
    use rusqlite::Connection;

    fn open_conn(db_path: &str) -> Connection {
        let conn = Connection::open(db_path).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=OFF;"
        ).unwrap();
        conn
    }

    fn create_pool(db_path: &str, size: u32) -> Pool<SqliteConnectionManager> {
        let mgr = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder()
            .max_size(size)
            .build(mgr)
            .unwrap();
        // Set PRAGMAs on each connection via a test get
        for _ in 0..size {
            let conn = pool.get().unwrap();
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 PRAGMA foreign_keys=OFF;"
            ).unwrap();
        }
        pool
    }

    pub async fn run(schema: Schema) -> BenchResult {
        let db_path = "bench_rusqlite.db";
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{db_path}-wal"));
        let _ = std::fs::remove_file(format!("{db_path}-shm"));

        let mut peak_rss = rss_mb();

        // ── Writes ──
        let db_path_owned = db_path.to_string();
        let create_sql = schema.create_sql().to_string();
        let insert_sql = schema.insert_sql().to_string();
        let s = schema;

        let start = Instant::now();
        tokio::task::spawn_blocking(move || {
            let conn = open_conn(&db_path_owned);
            conn.execute(&create_sql, []).unwrap();
            for i in 0..ITERATIONS {
                match s {
                    Schema::Kv => {
                        let (k, v) = kv_insert_data(i);
                        conn.execute(&insert_sql, rusqlite::params![k, v]).unwrap();
                    }
                    Schema::Users => {
                        let (id, name, email, bio, ts) = user_insert_data(i);
                        conn.execute(&insert_sql, rusqlite::params![id, name, email, bio, ts]).unwrap();
                    }
                }
            }
        }).await.unwrap();
        let writes_per_sec = ops_per_sec(ITERATIONS, start.elapsed());
        peak_rss = peak_rss.max(rss_mb());

        // ── Reads (single connection) ──
        let keys = random_keys(schema.key_prefix(), ITERATIONS);
        let db_path_owned = db_path.to_string();
        let select_sql = schema.select_sql().to_string();

        let start = Instant::now();
        tokio::task::spawn_blocking(move || {
            let conn = open_conn(&db_path_owned);
            let mut stmt = conn.prepare(&select_sql).unwrap();
            for key in &keys {
                let _val: String = stmt.query_row(rusqlite::params![key], |row| row.get(0)).unwrap();
            }
        }).await.unwrap();
        let reads_single_per_sec = ops_per_sec(ITERATIONS, start.elapsed());
        peak_rss = peak_rss.max(rss_mb());

        // ── Reads (pool) ──
        let pool = create_pool(db_path, POOL_SIZE as u32);
        let keys = Arc::new(random_keys(schema.key_prefix(), ITERATIONS));
        let chunk_size = ITERATIONS / TASKS;

        let start = Instant::now();
        let mut handles = Vec::new();
        for t in 0..TASKS {
            let pool = pool.clone();
            let keys = keys.clone();
            let sql = schema.select_sql().to_string();
            let offset = t * chunk_size;
            handles.push(tokio::spawn(async move {
                tokio::task::spawn_blocking(move || {
                    let conn = pool.get().unwrap();
                    let mut stmt = conn.prepare(&sql).unwrap();
                    for i in offset..offset + chunk_size {
                        let _val: String = stmt.query_row(rusqlite::params![&keys[i]], |row| row.get(0)).unwrap();
                    }
                }).await.unwrap();
            }));
        }
        for h in handles { h.await.unwrap(); }
        let reads_pool_per_sec = ops_per_sec(ITERATIONS, start.elapsed());
        peak_rss = peak_rss.max(rss_mb());

        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{db_path}-wal"));
        let _ = std::fs::remove_file(format!("{db_path}-shm"));

        BenchResult { writes_per_sec, reads_single_per_sec, reads_pool_per_sec, peak_rss_mb: peak_rss }
    }
}

// ─── libsql benchmarks ────────────────────────────────────────────

mod bench_libsql {
    use super::*;
    use libsql::params;

    async fn open_conn(db_path: &str) -> libsql::Connection {
        let db = libsql::Builder::new_local(db_path).build().await.unwrap();
        let conn = db.connect().unwrap();
        conn.execute("PRAGMA journal_mode=WAL", ()).await.unwrap();
        conn.execute("PRAGMA synchronous=NORMAL", ()).await.unwrap();
        conn.execute("PRAGMA foreign_keys=OFF", ()).await.unwrap();
        conn
    }

    pub async fn run(schema: Schema) -> BenchResult {
        let db_path = "bench_libsql.db";
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{db_path}-wal"));
        let _ = std::fs::remove_file(format!("{db_path}-shm"));

        let mut peak_rss = rss_mb();

        // ── Writes ──
        let db = libsql::Builder::new_local(db_path).build().await.unwrap();
        let conn = db.connect().unwrap();
        conn.execute("PRAGMA journal_mode=WAL", ()).await.unwrap();
        conn.execute("PRAGMA synchronous=NORMAL", ()).await.unwrap();
        conn.execute("PRAGMA foreign_keys=OFF", ()).await.unwrap();
        conn.execute(schema.create_sql(), ()).await.unwrap();

        let start = Instant::now();
        for i in 0..ITERATIONS {
            match schema {
                Schema::Kv => {
                    let (k, v) = kv_insert_data(i);
                    conn.execute(schema.insert_sql(), params![k, v]).await.unwrap();
                }
                Schema::Users => {
                    let (id, name, email, bio, ts) = user_insert_data(i);
                    conn.execute(schema.insert_sql(), params![id, name, email, bio, ts]).await.unwrap();
                }
            }
        }
        let writes_per_sec = ops_per_sec(ITERATIONS, start.elapsed());
        peak_rss = peak_rss.max(rss_mb());

        // ── Reads (single connection) ──
        let keys = random_keys(schema.key_prefix(), ITERATIONS);

        let start = Instant::now();
        for key in &keys {
            let mut rows = conn.query(schema.select_sql(), params![key.clone()]).await.unwrap();
            let _row = rows.next().await.unwrap().unwrap();
        }
        let reads_single_per_sec = ops_per_sec(ITERATIONS, start.elapsed());
        peak_rss = peak_rss.max(rss_mb());

        // ── Reads (pool of connections) ──
        let keys = Arc::new(random_keys(schema.key_prefix(), ITERATIONS));
        let chunk_size = ITERATIONS / TASKS;
        let db = Arc::new(db);

        let start = Instant::now();
        let mut handles = Vec::new();
        for t in 0..TASKS {
            let db = db.clone();
            let keys = keys.clone();
            let sql = schema.select_sql();
            let offset = t * chunk_size;
            handles.push(tokio::spawn(async move {
                let conn = db.connect().unwrap();
                conn.execute("PRAGMA journal_mode=WAL", ()).await.unwrap();
                conn.execute("PRAGMA synchronous=NORMAL", ()).await.unwrap();
                for i in offset..offset + chunk_size {
                    let mut rows = conn.query(sql, params![keys[i].clone()]).await.unwrap();
                    let _row = rows.next().await.unwrap().unwrap();
                }
            }));
        }
        for h in handles { h.await.unwrap(); }
        let reads_pool_per_sec = ops_per_sec(ITERATIONS, start.elapsed());
        peak_rss = peak_rss.max(rss_mb());

        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{db_path}-wal"));
        let _ = std::fs::remove_file(format!("{db_path}-shm"));

        BenchResult { writes_per_sec, reads_single_per_sec, reads_pool_per_sec, peak_rss_mb: peak_rss }
    }
}

// ─── Output formatting ────────────────────────────────────────────

fn print_results(schema: Schema, sqlx: &BenchResult, rusqlite: &BenchResult, libsql: &BenchResult) {
    println!("\nSchema: {}", schema.name());
    println!("{}", "─".repeat(72));
    println!(
        "  {:12} {:>12} {:>14} {:>16} {:>14}",
        "", "writes/s", "reads/s (1)", "reads/s (pool)", "peak RSS (MB)"
    );
    for (name, r) in [("sqlx", sqlx), ("rusqlite", rusqlite), ("libsql", libsql)] {
        println!(
            "  {:12} {:>12.0} {:>14.0} {:>16.0} {:>14.1}",
            name, r.writes_per_sec, r.reads_single_per_sec, r.reads_pool_per_sec, r.peak_rss_mb
        );
    }
}

// ─── Main ──────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    println!("SQLite Driver Benchmark ({ITERATIONS} iterations)");
    println!("{}", "=".repeat(72));

    for schema in [Schema::Kv, Schema::Users] {
        print!("\nBenchmarking sqlx ({})...", schema.name());
        let sqlx_result = bench_sqlx::run(schema).await;
        println!(" done");

        print!("Benchmarking rusqlite ({})...", schema.name());
        let rusqlite_result = bench_rusqlite::run(schema).await;
        println!(" done");

        print!("Benchmarking libsql ({})...", schema.name());
        let libsql_result = bench_libsql::run(schema).await;
        println!(" done");

        print_results(schema, &sqlx_result, &rusqlite_result, &libsql_result);
    }

    println!();
}
```

- [ ] **Step 2: Build and verify it compiles**

```bash
cd experiments/sqlite-bench && cargo build
```

Expected: Compiles with no errors. May have warnings about unused `open_conn` in libsql module — that's fine (it's used for reads).

- [ ] **Step 3: Run the benchmark**

```bash
cd experiments/sqlite-bench && cargo run --release
```

**IMPORTANT**: Must use `--release` for meaningful numbers. Debug builds have 10-100x overhead from bounds checks.

Expected: Runs all 18 benchmarks, prints two tables. Takes 1-5 minutes depending on hardware.

If any benchmark panics or errors, report the exact error message.

- [ ] **Step 4: Verify output format**

Output should look like:

```
SQLite Driver Benchmark (100000 iterations)
========================================================================

Benchmarking sqlx (kv (key TEXT, value TEXT))... done
Benchmarking rusqlite (kv (key TEXT, value TEXT))... done
Benchmarking libsql (kv (key TEXT, value TEXT))... done

Schema: kv (key TEXT, value TEXT)
────────────────────────────────────────────────────────────────────────
               writes/s   reads/s (1)  reads/s (pool)  peak RSS (MB)
  sqlx           NNNNN        NNNNN          NNNNN           NN.N
  rusqlite       NNNNN        NNNNN          NNNNN           NN.N
  libsql         NNNNN        NNNNN          NNNNN           NN.N

Benchmarking sqlx (users (id, name, email, bio, created_at))... done
...
```

Actual numbers will vary — the important thing is all 18 runs complete and produce nonzero values.

---

## Summary

| Task | What | Est. Time |
|------|------|-----------|
| 1 | Scaffold Cargo.toml + verify deps | 2 min |
| 2 | Full benchmark implementation + run | 10 min |
