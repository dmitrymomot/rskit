# modo-jobs Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement the `modo-jobs` crate — a DB-backed job queue with per-queue concurrency, priority ordering, retry backoff, auto-cleanup, and in-memory cron scheduling.

**Architecture:** Two new workspace crates: `modo-jobs-macros` (proc macro) and `modo-jobs` (runtime). modo-jobs takes a `DbPool` from modo-db, creates its entity table via schema sync, and spawns background tokio tasks for polling, reaping, and cleanup. Jobs are defined with `#[modo_jobs::job(...)]` and auto-registered via `inventory`.

**Tech Stack:** Rust, SeaORM (via modo-db), tokio (spawn, semaphore, timers, cancellation), inventory, cron, serde, tracing.

**Design doc:** `docs/plans/2026-03-07-modo-jobs-design.md`

**Conventions from codebase:**
- Config: `#[derive(Deserialize)] #[serde(default)]` structs with `Default` impl
- Entity: `#[modo_db::entity(table = "...")]` with `#[entity(timestamps)]`
- Extractor: `FromRequestParts<AppState>`, pulls from `ServiceRegistry`
- Error: `modo::Error` / `modo::error::Error`
- Tests: `cargo test --workspace`, integration tests in `crate/tests/*.rs`
- Format: `just fmt` before commit, `just check` for CI
- Workspace: members listed in root `Cargo.toml`

---

### Task 1: Scaffold `modo-jobs-macros` crate

**Files:**
- Create: `modo-jobs-macros/Cargo.toml`
- Create: `modo-jobs-macros/src/lib.rs`
- Create: `modo-jobs-macros/src/job.rs`
- Modify: `Cargo.toml` (workspace root)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "modo-jobs-macros"
version = "0.1.0"
edition = "2024"
license.workspace = true

[lib]
proc-macro = true

[dependencies]
syn = { version = "2", features = ["full", "extra-traits"] }
quote = "1"
proc-macro2 = "1"
```

**Step 2: Create lib.rs**

```rust
use proc_macro::TokenStream;

mod job;

#[proc_macro_attribute]
pub fn job(attr: TokenStream, item: TokenStream) -> TokenStream {
    match job::expand(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
```

**Step 3: Create stub job.rs**

```rust
use proc_macro2::TokenStream;
use syn::Result;

pub fn expand(_attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    Ok(item)
}
```

**Step 4: Add `"modo-jobs-macros"` to workspace members in root `Cargo.toml`**

**Step 5: Verify**

Run: `cargo check -p modo-jobs-macros`

**Step 6: Commit**

Message: `feat(modo-jobs-macros): scaffold proc macro crate`

---

### Task 2: Scaffold `modo-jobs` crate with config and types

**Files:**
- Create: `modo-jobs/Cargo.toml`
- Create: `modo-jobs/src/lib.rs`
- Create: `modo-jobs/src/config.rs`
- Create: `modo-jobs/src/types.rs`
- Modify: `Cargo.toml` (workspace root)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "modo-jobs"
version = "0.1.0"
edition = "2024"
license.workspace = true

[dependencies]
modo = { path = "../modo" }
modo-db = { path = "../modo-db" }
modo-jobs-macros = { path = "../modo-jobs-macros" }

inventory = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
cron = "0.15"
tokio = { version = "1", features = ["rt", "time", "sync", "macros"] }
tokio-util = { version = "0.7", features = ["rt"] }
tracing = "0.1"
ulid = "1"

[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
serde_yaml_ng = "0.10"
```

**Step 2: Create config.rs**

`JobsConfig` with `#[serde(default)]` and `Default` impl:
- `poll_interval_secs: u64` (default 1)
- `stale_threshold_secs: u64` (default 600)
- `drain_timeout_secs: u64` (default 30)
- `queues: Vec<QueueConfig>` (default: single "default" with concurrency 4)
- `cleanup: CleanupConfig`

`QueueConfig`: `name: String`, `concurrency: usize`

`CleanupConfig` with `#[serde(default)]`:
- `interval_secs: u64` (default 3600)
- `retention_secs: u64` (default 86400)
- `statuses: Vec<String>` (default: ["completed", "dead"])

**Step 3: Create types.rs**

- `JobId(String)` — ULID, `new()`, `as_str()`, `from_raw()`, Display, Default
- `JobState` — Pending/Running/Completed/Failed/Dead, Display, FromStr, `as_str()`

**Step 4: Create lib.rs**

Module declarations + public re-exports + re-exports for macro-generated code (`chrono`, `inventory`, `modo`, `serde_json`).

**Step 5: Add `"modo-jobs"` to workspace members**

**Step 6: Verify**

Run: `cargo check -p modo-jobs`

**Step 7: Commit**

Message: `feat(modo-jobs): scaffold crate with config and types`

---

### Task 3: Config and types tests

**Files:**
- Create: `modo-jobs/tests/config.rs`
- Create: `modo-jobs/tests/types.rs`

**Step 1: Write config tests**

- `test_jobs_config_defaults` — verify all default values
- `test_cleanup_config_defaults` — verify cleanup defaults
- `test_jobs_config_deserialize_yaml` — full YAML with custom values
- `test_jobs_config_partial_yaml_uses_defaults` — partial YAML, rest defaults

**Step 2: Write types tests**

- `test_job_id_unique` — two IDs are different
- `test_job_id_is_26_char_ulid` — length check
- `test_job_id_display` — Display matches as_str
- `test_job_state_display_roundtrip` — all states round-trip through Display/FromStr
- `test_job_state_from_str_invalid` — unknown string errors

**Step 3: Run tests**

Run: `cargo test -p modo-jobs`

**Step 4: Commit**

Message: `test(modo-jobs): add config and types tests`

---

### Task 4: Entity, handler trait, and JobQueue

**Files:**
- Create: `modo-jobs/src/entity.rs`
- Create: `modo-jobs/src/handler.rs`
- Create: `modo-jobs/src/queue.rs`
- Modify: `modo-jobs/src/lib.rs`

**Step 1: Create entity.rs**

Manual SeaORM entity (framework entity, same pattern as legacy). Table `modo_jobs` with columns: id, name, queue, payload, state, priority, attempts, max_retries, run_at (`ChronoDateTimeUtc`), timeout_secs, locked_by, locked_at (`Option<ChronoDateTimeUtc>`), created_at, updated_at.

Register via `inventory::submit!` with `is_framework: true`. Add composite index for claim query: `CREATE INDEX IF NOT EXISTS idx_modo_jobs_claim ON modo_jobs(state, queue, run_at, priority)`.

**Step 2: Create handler.rs**

- `JobHandler` trait — `fn run(&self, ctx: JobContext) -> impl Future<...>`
- `JobHandlerDyn` trait — object-safe version with `Pin<Box<dyn Future<...>>>`
- Blanket impl `JobHandlerDyn for T: JobHandler`
- `JobRegistration` struct — name, queue, priority, max_retries, timeout, cron, handler_factory
- `inventory::collect!(JobRegistration)`
- `JobContext` struct — job_id, name, queue, attempt, services (`ServiceRegistry`), db (`Option<DbPool>`), payload_json
- `JobContext` methods: `payload<T>()`, `service<T>()`, `db()`

**Step 3: Create queue.rs**

- `JobQueue` struct wrapping `Arc<DatabaseConnection>`
- `new(db: &DbPool)` constructor
- `enqueue<T: Serialize>(name, payload)` — finds registration, serializes payload, inserts entity
- `enqueue_at<T: Serialize>(name, payload, run_at)` — same with custom run_at
- `cancel(id)` — UPDATE pending -> dead, error if not found/not pending
- Private `insert_job()` helper

**Step 4: Update lib.rs with new modules and re-exports**

Re-export: `JobContext`, `JobHandler`, `JobHandlerDyn`, `JobRegistration`, `JobQueue`

**Step 5: Verify**

Run: `cargo check -p modo-jobs`

**Step 6: Commit**

Message: `feat(modo-jobs): add entity, handler trait, and JobQueue`

---

### Task 5: JobQueue axum extractor

**Files:**
- Create: `modo-jobs/src/extractor.rs`
- Modify: `modo-jobs/src/lib.rs`

**Step 1: Create extractor.rs**

Implement `FromRequestParts<AppState>` for `JobQueue`. Pull from `ServiceRegistry` via `state.services.get::<JobQueue>()`. Clone the inner `Arc`. Error if not registered.

**Step 2: Add `pub mod extractor;` to lib.rs**

**Step 3: Verify**

Run: `cargo check -p modo-jobs`

**Step 4: Commit**

Message: `feat(modo-jobs): add JobQueue axum extractor`

---

### Task 6: Runner — poll loop, claim, execute, retry

**Files:**
- Create: `modo-jobs/src/runner.rs`
- Modify: `modo-jobs/src/lib.rs`

This is the largest task. The runner contains:

**`JobsHandle`** — returned from `start()`, holds JobQueue + CancellationToken. Implements `Clone`, `Deref<Target = JobQueue>`. Has `shutdown()` method.

**`start(db, config, services)`** — validates queue config against registrations (panic on mismatch), spawns per-queue poll loops + stale reaper + cleanup + cron scheduler. Returns `JobsHandle`.

Note: `start()` takes `ServiceRegistry` as third arg (needed for DI in job handlers). The design doc shows `start(db, config)` but we need services for the JobContext.

**`poll_loop()`** — per-queue, owns a `Semaphore(concurrency)`. On each tick: try acquire permit, claim next job atomically, spawn execution task.

**`claim_next()`** — atomic single-statement SQL. Detects backend via `db.get_database_backend()`:
- SQLite: `UPDATE ... WHERE id = (SELECT ... LIMIT 1) RETURNING *`
- Postgres: same but with `FOR UPDATE SKIP LOCKED` in subquery

Parse the RETURNING row into `entity::Model` manually via `row.try_get()`.

**`execute_job()`** — build `JobContext`, find handler, run with `tokio::time::timeout()`. On success: `mark_completed()`. On failure with retries left: `mark_failed()` with exponential backoff (`5s * 2^(attempt-1)`, capped 1h). On failure exhausted: `mark_dead()`.

**Structured logging** — all failures logged with job_id, job_name, queue, attempt, max_retries, error.

**`mark_completed/failed/dead()`** — SeaORM `update_many` with `Expr::value()`.

**`reap_stale_loop()`** — every 60s, reset `running` jobs with `locked_at < now - threshold` back to `pending`.

**`cleanup_loop()`** — every `cleanup.interval_secs`, delete jobs matching `cleanup.statuses` with `updated_at < now - retention`.

**Graceful shutdown** — cancel token stops poll loops, drain waits up to `drain_timeout_secs` for in-flight jobs.

**Update lib.rs:** Add `pub mod runner;`, re-export `start` and `JobsHandle`.

**Verify:** `cargo check -p modo-jobs`

**Commit:** `feat(modo-jobs): add runner with poll loops, claim, retry, stale reaper, cleanup`

---

### Task 7: Cron scheduler

**Files:**
- Create: `modo-jobs/src/cron.rs`
- Modify: `modo-jobs/src/lib.rs`

**`start_cron_jobs(cancel, services, db)`** — iterate `JobRegistration` entries with `cron.is_some()`, parse cron expression (panic on invalid), spawn a task per cron job.

**`run_cron_loop()`** — sleep until next fire, build `JobContext` (with "cron" queue, attempt=1, Null payload), run handler with timeout. Log success/failure via tracing. Loop until cancelled.

**Add `pub(crate) mod cron;` to lib.rs** (called from runner's `start()`).

**Verify:** `cargo check -p modo-jobs`

**Commit:** `feat(modo-jobs): add cron scheduler with in-memory timers`

---

### Task 8: `#[job(...)]` proc macro

**Files:**
- Modify: `modo-jobs-macros/src/job.rs`

Replace the stub with the full implementation.

**Attribute parsing:**
- `queue` (String, default "default")
- `priority` (i32, default 0)
- `max_retries` (u32, default 3)
- `timeout` (duration string, default "5m")
- `cron` (String, optional)
- Mutual exclusion: cron + queue/priority/max_retries = compile error

**Parameter classification:** Same as legacy — `Payload(Type)`, `Service(Type)`, `Db`

**Code generation:**
- Rename original fn to `__job_{name}_impl` (private)
- Generate `{PascalName}Job` struct
- Implement `modo_jobs::JobHandler` — setup stmts extract payload/service/db from `JobContext`, call impl fn
- For queued jobs: generate `enqueue(&JobQueue, &Payload)` and `enqueue_at(&JobQueue, &Payload, DateTime)` — takes `&JobQueue` as first arg (no global)
- For cron jobs: no enqueue methods
- For no-payload jobs: enqueue takes only `&JobQueue`
- `inventory::submit!` with `JobRegistration`

**All generated code references `modo_jobs::` (not `modo::jobs::`)**. Db extraction references `modo_db::Db`.

**Verify:** `cargo check -p modo-jobs-macros`

**Commit:** `feat(modo-jobs-macros): implement #[job] proc macro with DI support`

---

### Task 9: Integration tests

**Files:**
- Create: `modo-jobs/tests/entity.rs`
- Create: `modo-jobs/tests/queue.rs`

**entity.rs tests:**
- `test_entity_table_created` — connect in-memory SQLite, sync, SELECT from modo_jobs
- `test_claim_index_created` — check sqlite_master for idx_modo_jobs_claim

**queue.rs tests:**
- Requires a registered job to test enqueue. Since we can't use the macro in tests easily (inventory registration is process-global), test the entity insertion directly via `JobQueue::insert_job()` or raw SeaORM inserts + verify query.

**Verify:** `cargo test -p modo-jobs`

**Commit:** `test(modo-jobs): add integration tests for entity and queue`

---

### Task 10: Run full workspace check

**Step 1:** `just fmt`
**Step 2:** `just check` (fmt-check + lint + test)
**Step 3:** Fix any clippy warnings or test failures.
**Step 4:** Commit fixes if any.

Message: `fix(modo-jobs): address clippy warnings`

---

### Task 11: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

Add modo-jobs to the architecture section as implemented. Add new commands/conventions. Note any gotchas discovered.

**Commit:** `docs: update CLAUDE.md with modo-jobs status`

---

## Summary

| Task | What | Estimated files |
|------|------|----------------|
| 1 | Scaffold modo-jobs-macros | 3 new |
| 2 | Scaffold modo-jobs with config/types | 4 new |
| 3 | Config and types tests | 2 new |
| 4 | Entity + handler + JobQueue | 3 new |
| 5 | JobQueue extractor | 1 new |
| 6 | Runner (largest task) | 1 new |
| 7 | Cron scheduler | 1 new |
| 8 | Job proc macro (full impl) | 1 modified |
| 9 | Integration tests | 2 new |
| 10 | Full workspace check | fixes only |
| 11 | Update CLAUDE.md | 1 modified |

**Critical path:** Tasks 1-8 are sequential. Tasks 9-11 after 8.

**Open question for Task 6:** `start()` needs `ServiceRegistry` for DI in job handlers. The design doc shows `start(db, config)` but implementation needs a third arg. Options:
1. `start(db, config, services)` — explicit, but user must pass ServiceRegistry before building the app
2. JobsHandle registers a `CancellationToken` as service, runner starts lazily when first job is claimed
3. Resolve during implementation — may need `AppBuilder` integration

Recommend option 1 for now, refactor if needed.
