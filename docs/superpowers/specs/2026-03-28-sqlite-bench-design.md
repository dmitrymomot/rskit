# SQLite Driver Benchmark Experiment

**Date**: 2026-03-28
**Goal**: Benchmark sqlx vs rusqlite vs libsql across writes, single-connection reads, and pooled reads on two schemas. Measure ops/sec and peak memory usage.

## Context

modo is evaluating replacing sqlx with libsql (or rusqlite). Before committing, we need hard numbers on driver overhead for the operations that matter: single-row writes, single-connection reads, and concurrent pooled reads. All three drivers use the same underlying C SQLite engine — this benchmark isolates the Rust driver layer overhead.

## Success Criteria

1. All 18 benchmark runs complete without error (3 drivers x 2 schemas x 3 scenarios)
2. Output a formatted comparison table with ops/sec and peak RSS
3. Results are reproducible (variance < 10% across runs)

## File Structure

```
experiments/
└── sqlite-bench/
    ├── Cargo.toml
    └── src/
        └── main.rs
```

Single binary, single file. Throwaway benchmark, not a library.

## Schemas

### Simple KV

```sql
CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT)
```

Insert: `INSERT INTO kv (key, value) VALUES (?1, ?2)` with `key_N` / `value_N`.

Read: `SELECT key, value FROM kv WHERE key = ?1` with random key from inserted set.

### Realistic Users

```sql
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    bio TEXT NOT NULL,
    created_at TEXT NOT NULL
)
```

Insert: `INSERT INTO users (id, name, email, bio, created_at) VALUES (?1, ?2, ?3, ?4, ?5)` with generated data.

Read: `SELECT id, name, email, bio, created_at FROM users WHERE id = ?1` with random id from inserted set.

## Benchmark Matrix

| Driver | Write (INSERT) | Read 1-conn | Read pool |
|--------|---------------|-------------|-----------|
| sqlx | 100k sequential inserts, 1 pool connection | 100k SELECTs by random PK, 1 pool connection | 100k SELECTs, 10 pool connections, 10 concurrent tokio tasks |
| rusqlite | 100k sequential inserts, 1 connection, each wrapped in spawn_blocking | 100k SELECTs, 1 connection, each wrapped in spawn_blocking | 100k SELECTs, r2d2 pool of 10, 10 concurrent tokio tasks with spawn_blocking |
| libsql | 100k sequential inserts, 1 connection | 100k SELECTs, 1 connection | 100k SELECTs, 10 connections, 10 concurrent tokio tasks |

Each cell runs on both schemas = **18 benchmark runs** total.

## Configuration

- **Iterations**: 100,000 per benchmark run
- **Pool size**: 10 connections (for pooled read benchmark)
- **Concurrent tasks**: 10 (for pooled read benchmark, each task runs iterations/10 queries)
- **WAL mode**: Enabled for all three drivers
- **Database files**: Separate per driver (`bench_sqlx.db`, `bench_rusqlite.db`, `bench_libsql.db`), deleted and recreated for each schema
- **PRAGMAs**: `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=OFF` — same for all drivers

## Methodology

### Write Benchmark

1. Create fresh database with schema
2. Record start time + RSS
3. INSERT 100k rows sequentially (no transaction batching — measures per-row write overhead)
4. Record end time + peak RSS
5. Calculate ops/sec

### Read 1-Connection Benchmark

1. Database already has 100k rows from write benchmark
2. Pre-generate 100k random keys from the inserted set
3. Record start time + RSS
4. SELECT by random PK, 100k times, single connection
5. Record end time + peak RSS
6. Calculate ops/sec

### Read Pool Benchmark

1. Database already has 100k rows
2. Pre-generate 100k random keys
3. Create pool of 10 connections
4. Record start time + RSS
5. Spawn 10 tokio tasks, each runs 10k SELECTs using a connection from the pool
6. Await all tasks
7. Record end time + peak RSS
8. Calculate ops/sec

### rusqlite async pattern

All rusqlite operations use `tokio::task::spawn_blocking` to simulate realistic async usage (since rusqlite is synchronous). For the pool benchmark, each task acquires from r2d2 inside spawn_blocking.

## Memory Measurement

Peak RSS measured via macOS `task_info()` syscall (`mach_task_basic_info`). Captured before and after each benchmark suite per driver+schema. Report peak value observed.

No external dependencies needed — direct syscall via `libc`/`mach` FFI.

## Output Format

```
SQLite Driver Benchmark (100,000 iterations)
============================================

Schema: kv (key TEXT, value TEXT)
─────────────────────────────────────────────────────────────────
               writes/s    reads/s (1)  reads/s (pool)  peak RSS (MB)
  sqlx         12,345      45,678       89,012          24.5
  rusqlite     15,678      56,789       98,765          18.2
  libsql       14,567      50,123       95,432          20.1

Schema: users (id, name, email, bio, created_at)
─────────────────────────────────────────────────────────────────
               writes/s    reads/s (1)  reads/s (pool)  peak RSS (MB)
  sqlx         11,234      40,567       80,123          26.3
  rusqlite     14,567      51,234       90,456          19.8
  libsql       13,456      47,890       88,765          22.1
```

(Numbers above are illustrative placeholders — actual results will vary.)

## Dependencies

```toml
[package]
name = "sqlite-bench"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"], default-features = false }
rusqlite = { version = "0.34", features = ["bundled"] }
r2d2 = "0.8"
r2d2_sqlite = "0.25"
libsql = "0.9"
rand = "0.9"
```

## Out of Scope

- Transaction batching (real apps batch — this measures worst-case per-row)
- Vector search performance (separate concern, already validated in libsql experiment)
- Concurrent write contention (SQLite serializes writes regardless of driver)
- Latency percentiles (ops/sec is sufficient for driver comparison)
- Cross-platform support (macOS only for RSS measurement)
