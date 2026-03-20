# modo v2 Job Queue & Cron Scheduler Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build two modules — `job` (DB-backed background job queue with extractor-based handlers) and `cron` (in-memory recurring task scheduler) — for modo v2.

**Architecture:** The `job` module provides an `Enqueuer` (inserts rows) and a `Worker` (polls, claims, executes with retry/backoff). The `cron` module provides a `Scheduler` with per-job tokio loops. Both use axum-style extractor traits (`FromJobContext`/`FromCronContext`) with blanket impls over async fn signatures. A prerequisite task adds `Registry::snapshot()` so Worker/Scheduler can hold an `Arc<RegistrySnapshot>` without consuming the registry. Handler registration uses a builder pattern: `Worker::builder().register().start()`.

**Important notes:**
- Rust 2024 edition: `std::env::set_var`/`remove_var` are `unsafe` — all tests wrap in `unsafe {}` blocks
- File organization: `mod.rs` is ONLY for `mod` imports and re-exports — all code in separate files
- All SQL timestamps computed in Rust as RFC 3339 strings, bound as parameters — no `now()` in SQL
- String length checks must use `.chars().count()`, not `.len()`
- Use official documentation only when researching dependencies
- `pub(crate)` items cannot be tested from integration tests (`tests/*.rs`) — use `#[cfg(test)] mod tests` inside the source file instead
- Tests that modify env vars must clean up BEFORE assertions and use `serial_test` crate
- Tracing fields always snake_case

**Tech Stack:** Rust 2024 edition, axum 0.8, sqlx 0.8 (SQLite), tokio 1, tokio-util (CancellationToken), sha2 0.10, chrono 0.4, croner (cron expressions), serde/serde_json.

**Spec:** `docs/superpowers/specs/2026-03-20-modo-v2-job-cron-design.md`

---

## File Structure

```
Cargo.toml                      -- MODIFY: add tokio-util, croner
src/
  lib.rs                        -- MODIFY: add job, cron modules + re-exports
  service/
    registry.rs                 -- MODIFY: add snapshot() method
    snapshot.rs                 -- CREATE: RegistrySnapshot type
    mod.rs                      -- MODIFY: add snapshot module + re-export
  config/
    modo.rs                     -- MODIFY: add job config field
  job/
    mod.rs                      -- mod imports + pub use re-exports
    config.rs                   -- JobConfig, QueueConfig, CleanupConfig
    meta.rs                     -- Status enum, Meta struct
    payload.rs                  -- Payload<T> extractor type
    context.rs                  -- JobContext (pub(crate)), FromJobContext trait + impls
    handler.rs                  -- JobHandler trait + blanket impls macro
    enqueuer.rs                 -- Enqueuer, EnqueueOptions, EnqueueResult
    worker.rs                   -- Worker, WorkerBuilder, JobOptions
    reaper.rs                   -- stale reaper loop
    cleanup.rs                  -- cleanup loop
  cron/
    mod.rs                      -- mod imports + pub use re-exports
    meta.rs                     -- cron::Meta
    schedule.rs                 -- schedule parsing (@every, aliases, standard cron)
    context.rs                  -- CronContext (pub(crate)), FromCronContext trait + impls
    handler.rs                  -- CronHandler trait + blanket impls macro
    scheduler.rs                -- Scheduler, SchedulerBuilder, CronOptions
tests/
  job_config_test.rs            -- JobConfig deserialization tests
  job_enqueuer_test.rs          -- Enqueuer CRUD tests (SQLite in-memory)
  job_handler_test.rs           -- JobHandler trait + extractor tests
  job_worker_test.rs            -- Worker polling, claim, retry, timeout tests
  cron_schedule_test.rs         -- schedule parsing tests
  cron_handler_test.rs          -- CronHandler trait + extractor tests
  cron_scheduler_test.rs        -- Scheduler execution, overlap, shutdown tests
```

---

### Task 1: Add dependencies to Cargo.toml

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add tokio-util and croner to dependencies**

Add to `[dependencies]` section after the existing `tokio` entry:

```toml
tokio-util = { version = "0.7", features = ["rt"] }
croner = "2"
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "deps: add tokio-util and croner for job/cron modules"
```

---

### Task 2: Add RegistrySnapshot to service module

**Files:**
- Create: `src/service/snapshot.rs`
- Modify: `src/service/registry.rs`
- Modify: `src/service/mod.rs`

- [ ] **Step 1: Create RegistrySnapshot type**

Create `src/service/snapshot.rs`:

```rust
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct RegistrySnapshot {
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl RegistrySnapshot {
    pub(crate) fn new(services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>) -> Self {
        Self { services }
    }

    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|arc| arc.clone().downcast::<T>().ok())
    }
}
```

- [ ] **Step 2: Add snapshot() to Registry**

In `src/service/registry.rs`, add method to `impl Registry`:

```rust
pub(crate) fn snapshot(&self) -> Arc<super::RegistrySnapshot> {
    Arc::new(super::RegistrySnapshot::new(self.services.clone()))
}
```

- [ ] **Step 3: Update service mod.rs**

In `src/service/mod.rs`, add:

```rust
mod snapshot;
pub(crate) use snapshot::RegistrySnapshot;
```

Note: `RegistrySnapshot` is `pub(crate)` — only used internally by job/cron modules.

- [ ] **Step 4: Add inline test for snapshot**

In `src/service/snapshot.rs`, add at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_retrieves_stored_service() {
        let mut map = HashMap::new();
        map.insert(TypeId::of::<u32>(), Arc::new(42u32) as Arc<dyn Any + Send + Sync>);
        let snap = RegistrySnapshot::new(map);
        let val = snap.get::<u32>().unwrap();
        assert_eq!(*val, 42);
    }

    #[test]
    fn snapshot_returns_none_for_missing() {
        let snap = RegistrySnapshot::new(HashMap::new());
        assert!(snap.get::<String>().is_none());
    }
}
```

- [ ] **Step 5: Verify tests pass**

Run: `cargo test --lib service::snapshot`
Expected: 2 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/service/snapshot.rs src/service/registry.rs src/service/mod.rs
git commit -m "feat(service): add RegistrySnapshot for job/cron context sharing"
```

---

### Task 3: Job config

**Files:**
- Create: `src/job/mod.rs`
- Create: `src/job/config.rs`
- Modify: `src/config/modo.rs`
- Modify: `src/lib.rs`
- Create: `tests/job_config_test.rs`

- [ ] **Step 1: Create job/mod.rs skeleton**

Create `src/job/mod.rs`:

```rust
mod config;

pub use config::{CleanupConfig, JobConfig, QueueConfig};
```

- [ ] **Step 2: Create job/config.rs**

Create `src/job/config.rs`:

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct JobConfig {
    pub poll_interval_secs: u64,
    pub stale_threshold_secs: u64,
    pub stale_reaper_interval_secs: u64,
    pub drain_timeout_secs: u64,
    pub queues: Vec<QueueConfig>,
    pub cleanup: Option<CleanupConfig>,
}

impl Default for JobConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 1,
            stale_threshold_secs: 600,
            stale_reaper_interval_secs: 60,
            drain_timeout_secs: 30,
            queues: vec![QueueConfig::default()],
            cleanup: Some(CleanupConfig::default()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueConfig {
    pub name: String,
    pub concurrency: u32,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            concurrency: 4,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CleanupConfig {
    pub interval_secs: u64,
    pub retention_secs: u64,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            interval_secs: 3600,
            retention_secs: 259_200, // 72h
        }
    }
}
```

- [ ] **Step 3: Add job module to lib.rs**

In `src/lib.rs`, add:

```rust
pub mod job;
```

- [ ] **Step 4: Add job config to modo config**

In `src/config/modo.rs`, add field to `Config`:

```rust
pub job: crate::job::JobConfig,
```

- [ ] **Step 5: Write config test**

Create `tests/job_config_test.rs`:

```rust
use modo::job::JobConfig;

#[test]
fn default_config_has_sensible_values() {
    let config = JobConfig::default();
    assert_eq!(config.poll_interval_secs, 1);
    assert_eq!(config.stale_threshold_secs, 600);
    assert_eq!(config.stale_reaper_interval_secs, 60);
    assert_eq!(config.drain_timeout_secs, 30);
    assert_eq!(config.queues.len(), 1);
    assert_eq!(config.queues[0].name, "default");
    assert_eq!(config.queues[0].concurrency, 4);
    let cleanup = config.cleanup.as_ref().unwrap();
    assert_eq!(cleanup.interval_secs, 3600);
    assert_eq!(cleanup.retention_secs, 259_200);
}

#[test]
fn deserializes_from_yaml() {
    let yaml = r#"
poll_interval_secs: 2
stale_threshold_secs: 300
queues:
  - name: default
    concurrency: 8
  - name: email
    concurrency: 2
cleanup:
  interval_secs: 1800
  retention_secs: 86400
"#;
    let config: JobConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(config.poll_interval_secs, 2);
    assert_eq!(config.queues.len(), 2);
    assert_eq!(config.queues[1].name, "email");
    assert_eq!(config.cleanup.as_ref().unwrap().retention_secs, 86400);
}

#[test]
fn cleanup_null_disables_cleanup() {
    let yaml = r#"
cleanup: null
"#;
    let config: JobConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert!(config.cleanup.is_none());
}
```

- [ ] **Step 6: Verify**

Run: `cargo test --test job_config_test`
Expected: 3 tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/job/ src/lib.rs src/config/modo.rs tests/job_config_test.rs
git commit -m "feat(job): add JobConfig with queue and cleanup settings"
```

---

### Task 4: Job meta types (Status, Meta)

**Files:**
- Create: `src/job/meta.rs`
- Modify: `src/job/mod.rs`

- [ ] **Step 1: Create job/meta.rs**

Create `src/job/meta.rs`:

```rust
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Pending,
    Running,
    Completed,
    Dead,
    Cancelled,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Dead => "dead",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "dead" => Some(Self::Dead),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Dead | Self::Cancelled)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct Meta {
    pub id: String,
    pub name: String,
    pub queue: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub deadline: Option<tokio::time::Instant>,
}
```

- [ ] **Step 2: Add inline tests**

At bottom of `src/job/meta.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrip() {
        let statuses = [
            Status::Pending,
            Status::Running,
            Status::Completed,
            Status::Dead,
            Status::Cancelled,
        ];
        for s in &statuses {
            let parsed = Status::from_str(s.as_str()).unwrap();
            assert_eq!(&parsed, s);
        }
    }

    #[test]
    fn status_unknown_returns_none() {
        assert!(Status::from_str("unknown").is_none());
    }

    #[test]
    fn terminal_states() {
        assert!(!Status::Pending.is_terminal());
        assert!(!Status::Running.is_terminal());
        assert!(Status::Completed.is_terminal());
        assert!(Status::Dead.is_terminal());
        assert!(Status::Cancelled.is_terminal());
    }
}
```

- [ ] **Step 3: Update job/mod.rs**

Add to `src/job/mod.rs`:

```rust
mod meta;

pub use meta::{Meta, Status};
```

- [ ] **Step 4: Verify**

Run: `cargo test --lib job::meta`
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/job/meta.rs src/job/mod.rs
git commit -m "feat(job): add Status enum and Meta struct"
```

---

### Task 5: Job Payload extractor type

**Files:**
- Create: `src/job/payload.rs`
- Modify: `src/job/mod.rs`

- [ ] **Step 1: Create job/payload.rs**

Create `src/job/payload.rs`:

```rust
use std::ops::Deref;

pub struct Payload<T>(pub T);

impl<T> Deref for Payload<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
```

- [ ] **Step 2: Update job/mod.rs**

Add to `src/job/mod.rs`:

```rust
mod payload;

pub use payload::Payload;
```

- [ ] **Step 3: Verify**

Run: `cargo check`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add src/job/payload.rs src/job/mod.rs
git commit -m "feat(job): add Payload<T> extractor type"
```

---

### Task 6: JobContext, FromJobContext trait, and extractor impls

**Files:**
- Create: `src/job/context.rs`
- Modify: `src/job/mod.rs`

- [ ] **Step 1: Create job/context.rs**

Create `src/job/context.rs`:

```rust
use std::sync::Arc;

use serde::de::DeserializeOwned;

use crate::error::{Error, Result};
use crate::extractor::Service;
use crate::service::RegistrySnapshot;

use super::meta::Meta;
use super::payload::Payload;

pub(crate) struct JobContext {
    pub(crate) registry: Arc<RegistrySnapshot>,
    pub(crate) payload: String,
    pub(crate) meta: Meta,
}

pub trait FromJobContext: Sized {
    fn from_job_context(ctx: &JobContext) -> Result<Self>;
}

impl<T: DeserializeOwned> FromJobContext for Payload<T> {
    fn from_job_context(ctx: &JobContext) -> Result<Self> {
        let value: T = serde_json::from_str(&ctx.payload).map_err(|e| {
            Error::internal(format!(
                "failed to deserialize job payload for '{}': {e}",
                ctx.meta.name
            ))
        })?;
        Ok(Payload(value))
    }
}

impl<T: Send + Sync + 'static> FromJobContext for Service<T> {
    fn from_job_context(ctx: &JobContext) -> Result<Self> {
        ctx.registry.get::<T>().map(Service).ok_or_else(|| {
            Error::internal(format!(
                "service not found in registry: {}",
                std::any::type_name::<T>()
            ))
        })
    }
}

impl FromJobContext for Meta {
    fn from_job_context(ctx: &JobContext) -> Result<Self> {
        Ok(ctx.meta.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::{Any, TypeId};
    use std::collections::HashMap;

    fn test_context(payload: &str) -> JobContext {
        let mut services: HashMap<TypeId, Arc<dyn Any + Send + Sync>> = HashMap::new();
        services.insert(TypeId::of::<String>(), Arc::new("test-service".to_string()));
        let snapshot = Arc::new(RegistrySnapshot::new(services));

        JobContext {
            registry: snapshot,
            payload: payload.to_string(),
            meta: Meta {
                id: "test-id".to_string(),
                name: "test-job".to_string(),
                queue: "default".to_string(),
                attempt: 1,
                max_attempts: 3,
                deadline: None,
            },
        }
    }

    #[test]
    fn payload_extractor_deserializes_json() {
        let ctx = test_context(r#"{"value": 42}"#);

        #[derive(serde::Deserialize)]
        struct TestPayload {
            value: u32,
        }

        let payload = Payload::<TestPayload>::from_job_context(&ctx).unwrap();
        assert_eq!(payload.value, 42);
    }

    #[test]
    fn payload_extractor_fails_on_invalid_json() {
        let ctx = test_context("not json");
        let result = Payload::<serde_json::Value>::from_job_context(&ctx);
        assert!(result.is_err());
    }

    #[test]
    fn service_extractor_finds_registered() {
        let ctx = test_context("{}");
        let svc = Service::<String>::from_job_context(&ctx).unwrap();
        assert_eq!(*svc.0, "test-service");
    }

    #[test]
    fn service_extractor_fails_for_missing() {
        let ctx = test_context("{}");
        let result = Service::<u64>::from_job_context(&ctx);
        assert!(result.is_err());
    }

    #[test]
    fn meta_extractor_clones_meta() {
        let ctx = test_context("{}");
        let meta = Meta::from_job_context(&ctx).unwrap();
        assert_eq!(meta.id, "test-id");
        assert_eq!(meta.name, "test-job");
        assert_eq!(meta.attempt, 1);
    }
}
```

- [ ] **Step 2: Update job/mod.rs**

Add to `src/job/mod.rs`:

```rust
mod context;

pub use context::FromJobContext;
pub(crate) use context::JobContext;
```

- [ ] **Step 3: Verify**

Run: `cargo test --lib job::context`
Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/job/context.rs src/job/mod.rs
git commit -m "feat(job): add JobContext and FromJobContext extractors"
```

---

### Task 7: JobHandler trait with blanket impls

**Files:**
- Create: `src/job/handler.rs`
- Modify: `src/job/mod.rs`
- Create: `tests/job_handler_test.rs`

- [ ] **Step 1: Create job/handler.rs**

Create `src/job/handler.rs`:

```rust
use std::future::Future;

use crate::error::Result;

use super::context::{FromJobContext, JobContext};

pub trait JobHandler<Args>: Clone + Send + 'static {
    fn call(self, ctx: JobContext) -> impl Future<Output = Result<()>> + Send;
}

// 0 args
impl<F, Fut> JobHandler<()> for F
where
    F: FnOnce() -> Fut + Clone + Send + 'static,
    Fut: Future<Output = Result<()>> + Send,
{
    async fn call(self, _ctx: JobContext) -> Result<()> {
        (self)().await
    }
}

macro_rules! impl_job_handler {
    ($($T:ident),+) => {
        impl<F, Fut, $($T),+> JobHandler<($($T,)+)> for F
        where
            F: FnOnce($($T),+) -> Fut + Clone + Send + 'static,
            Fut: Future<Output = Result<()>> + Send,
            $($T: FromJobContext,)+
        {
            async fn call(self, ctx: JobContext) -> Result<()> {
                $(let $T = $T::from_job_context(&ctx)?;)+
                (self)($($T),+).await
            }
        }
    };
}

impl_job_handler!(T1);
impl_job_handler!(T1, T2);
impl_job_handler!(T1, T2, T3);
impl_job_handler!(T1, T2, T3, T4);
impl_job_handler!(T1, T2, T3, T4, T5);
impl_job_handler!(T1, T2, T3, T4, T5, T6);
impl_job_handler!(T1, T2, T3, T4, T5, T6, T7);
impl_job_handler!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_job_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_job_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_job_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_job_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
```

- [ ] **Step 2: Update job/mod.rs**

Add to `src/job/mod.rs`:

```rust
mod handler;

pub use handler::JobHandler;
```

- [ ] **Step 3: Write handler test**

Create `tests/job_handler_test.rs`:

```rust
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use modo::error::Result;
use modo::job::{JobHandler, Meta, Payload};
use modo::Service;

// Helper: build a JobContext from the internal types
// We can't construct JobContext directly (pub(crate)), so we test
// via the public handler trait interface. We need to verify that
// async fn with the right extractors satisfies JobHandler.
// Compile-time check is sufficient — if it compiles, the blanket impl works.

// Zero-arg handler
async fn noop_handler() -> Result<()> {
    Ok(())
}

// One-arg handler
async fn payload_handler(_payload: Payload<serde_json::Value>) -> Result<()> {
    Ok(())
}

// Two-arg handler
async fn two_arg_handler(
    _payload: Payload<serde_json::Value>,
    _meta: Meta,
) -> Result<()> {
    Ok(())
}

// Three-arg handler with Service
async fn full_handler(
    _payload: Payload<serde_json::Value>,
    _meta: Meta,
    _svc: Service<String>,
) -> Result<()> {
    Ok(())
}

// Compile-time assertions: these functions accept JobHandler, so if the
// async fns satisfy the trait bounds, this compiles.
fn assert_job_handler<H: JobHandler<Args>, Args>(_h: H) {}

#[test]
fn zero_arg_handler_satisfies_trait() {
    assert_job_handler(noop_handler);
}

#[test]
fn one_arg_handler_satisfies_trait() {
    assert_job_handler(payload_handler);
}

#[test]
fn two_arg_handler_satisfies_trait() {
    assert_job_handler(two_arg_handler);
}

#[test]
fn three_arg_handler_satisfies_trait() {
    assert_job_handler(full_handler);
}
```

- [ ] **Step 4: Verify**

Run: `cargo test --test job_handler_test`
Expected: 4 tests pass (compile-time trait satisfaction).

- [ ] **Step 5: Commit**

```bash
git add src/job/handler.rs src/job/mod.rs tests/job_handler_test.rs
git commit -m "feat(job): add JobHandler trait with blanket impls for 0..12 extractors"
```

---

### Task 8: Enqueuer

**Files:**
- Create: `src/job/enqueuer.rs`
- Modify: `src/job/mod.rs`
- Create: `tests/job_enqueuer_test.rs`

- [ ] **Step 1: Create job/enqueuer.rs**

Create `src/job/enqueuer.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::db::{InnerPool, Writer};
use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueResult {
    Created(String),
    Duplicate(String),
}

pub struct EnqueueOptions {
    pub queue: String,
    pub run_at: Option<DateTime<Utc>>,
}

impl Default for EnqueueOptions {
    fn default() -> Self {
        Self {
            queue: "default".to_string(),
            run_at: None,
        }
    }
}

#[derive(Clone)]
pub struct Enqueuer {
    writer: InnerPool,
}

impl Enqueuer {
    pub fn new(writer: &impl Writer) -> Self {
        Self {
            writer: writer.write_pool().clone(),
        }
    }

    pub async fn enqueue<T: Serialize>(
        &self,
        name: &str,
        payload: &T,
    ) -> Result<String> {
        self.enqueue_with(name, payload, EnqueueOptions::default()).await
    }

    pub async fn enqueue_at<T: Serialize>(
        &self,
        name: &str,
        payload: &T,
        run_at: DateTime<Utc>,
    ) -> Result<String> {
        self.enqueue_with(name, payload, EnqueueOptions {
            run_at: Some(run_at),
            ..Default::default()
        }).await
    }

    pub async fn enqueue_with<T: Serialize>(
        &self,
        name: &str,
        payload: &T,
        options: EnqueueOptions,
    ) -> Result<String> {
        let id = crate::id::ulid();
        let payload_json = serde_json::to_string(payload)
            .map_err(|e| Error::internal(format!("serialize job payload: {e}")))?;
        let now = Utc::now();
        let run_at = options.run_at.unwrap_or(now);
        let now_str = now.to_rfc3339();
        let run_at_str = run_at.to_rfc3339();

        sqlx::query(
            "INSERT INTO modo_jobs (id, name, queue, payload, status, attempt, run_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 'pending', 0, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(&options.queue)
        .bind(&payload_json)
        .bind(&run_at_str)
        .bind(&now_str)
        .bind(&now_str)
        .execute(&self.writer)
        .await
        .map_err(|e| Error::internal(format!("enqueue job: {e}")))?;

        Ok(id)
    }

    pub async fn enqueue_unique<T: Serialize>(
        &self,
        name: &str,
        payload: &T,
    ) -> Result<EnqueueResult> {
        let payload_json = serde_json::to_string(payload)
            .map_err(|e| Error::internal(format!("serialize job payload: {e}")))?;
        let hash = compute_payload_hash(name, &payload_json);

        // Check for existing pending/running job with same hash
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM modo_jobs WHERE payload_hash = ? AND status IN ('pending', 'running') LIMIT 1",
        )
        .bind(&hash)
        .fetch_optional(&self.writer)
        .await
        .map_err(|e| Error::internal(format!("check job uniqueness: {e}")))?;

        if let Some((existing_id,)) = existing {
            return Ok(EnqueueResult::Duplicate(existing_id));
        }

        let id = crate::id::ulid();
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        sqlx::query(
            "INSERT INTO modo_jobs (id, name, queue, payload, payload_hash, status, attempt, run_at, created_at, updated_at) \
             VALUES (?, ?, 'default', ?, ?, 'pending', 0, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(&payload_json)
        .bind(&hash)
        .bind(&now_str)
        .bind(&now_str)
        .bind(&now_str)
        .execute(&self.writer)
        .await
        .map_err(|e| Error::internal(format!("enqueue unique job: {e}")))?;

        Ok(EnqueueResult::Created(id))
    }

    pub async fn cancel(&self, id: &str) -> Result<bool> {
        let now_str = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE modo_jobs SET status = 'cancelled', updated_at = ? WHERE id = ? AND status = 'pending'",
        )
        .bind(&now_str)
        .bind(id)
        .execute(&self.writer)
        .await
        .map_err(|e| Error::internal(format!("cancel job: {e}")))?;

        Ok(result.rows_affected() > 0)
    }
}

fn compute_payload_hash(name: &str, payload_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.update(payload_json.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_hash_is_deterministic() {
        let h1 = compute_payload_hash("test", r#"{"a":1}"#);
        let h2 = compute_payload_hash("test", r#"{"a":1}"#);
        assert_eq!(h1, h2);
    }

    #[test]
    fn payload_hash_differs_by_name() {
        let h1 = compute_payload_hash("job_a", r#"{"a":1}"#);
        let h2 = compute_payload_hash("job_b", r#"{"a":1}"#);
        assert_ne!(h1, h2);
    }

    #[test]
    fn payload_hash_differs_by_payload() {
        let h1 = compute_payload_hash("test", r#"{"a":1}"#);
        let h2 = compute_payload_hash("test", r#"{"a":2}"#);
        assert_ne!(h1, h2);
    }
}
```

- [ ] **Step 2: Update job/mod.rs**

Add to `src/job/mod.rs`:

```rust
mod enqueuer;

pub use enqueuer::{EnqueueOptions, EnqueueResult, Enqueuer};
```

- [ ] **Step 3: Write enqueuer integration test**

Create `tests/job_enqueuer_test.rs`:

```rust
use chrono::{Duration, Utc};
use modo::db;
use modo::job::{EnqueueOptions, EnqueueResult, Enqueuer};
use serde::Serialize;

const CREATE_TABLE: &str = "
CREATE TABLE modo_jobs (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    queue         TEXT NOT NULL DEFAULT 'default',
    payload       TEXT NOT NULL DEFAULT '{}',
    payload_hash  TEXT,
    status        TEXT NOT NULL DEFAULT 'pending',
    attempt       INTEGER NOT NULL DEFAULT 0,
    run_at        TEXT NOT NULL,
    started_at    TEXT,
    completed_at  TEXT,
    failed_at     TEXT,
    error_message TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
)";

async fn setup() -> (Enqueuer, db::Pool) {
    let config = db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = db::connect(&config).await.unwrap();
    sqlx::query(CREATE_TABLE).execute(&*pool).await.unwrap();
    let enqueuer = Enqueuer::new(&pool);
    (enqueuer, pool)
}

#[derive(Serialize)]
struct TestPayload {
    user_id: String,
}

#[tokio::test]
async fn enqueue_inserts_pending_job() {
    let (enqueuer, pool) = setup().await;
    let id = enqueuer
        .enqueue("send_email", &TestPayload { user_id: "u1".into() })
        .await
        .unwrap();

    let row: (String, String, String, i64) =
        sqlx::query_as("SELECT name, queue, status, attempt FROM modo_jobs WHERE id = ?")
            .bind(&id)
            .fetch_one(&*pool)
            .await
            .unwrap();

    assert_eq!(row.0, "send_email");
    assert_eq!(row.1, "default");
    assert_eq!(row.2, "pending");
    assert_eq!(row.3, 0);
}

#[tokio::test]
async fn enqueue_at_sets_future_run_at() {
    let (enqueuer, pool) = setup().await;
    let future = Utc::now() + Duration::hours(1);
    let id = enqueuer
        .enqueue_at("report", &TestPayload { user_id: "u1".into() }, future)
        .await
        .unwrap();

    let (run_at_str,): (String,) =
        sqlx::query_as("SELECT run_at FROM modo_jobs WHERE id = ?")
            .bind(&id)
            .fetch_one(&*pool)
            .await
            .unwrap();

    let run_at = chrono::DateTime::parse_from_rfc3339(&run_at_str).unwrap();
    assert!(run_at > Utc::now());
}

#[tokio::test]
async fn enqueue_with_custom_queue() {
    let (enqueuer, pool) = setup().await;
    let id = enqueuer
        .enqueue_with(
            "send_email",
            &TestPayload { user_id: "u1".into() },
            EnqueueOptions {
                queue: "email".to_string(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let (queue,): (String,) =
        sqlx::query_as("SELECT queue FROM modo_jobs WHERE id = ?")
            .bind(&id)
            .fetch_one(&*pool)
            .await
            .unwrap();

    assert_eq!(queue, "email");
}

#[tokio::test]
async fn enqueue_unique_creates_first_time() {
    let (enqueuer, _pool) = setup().await;
    let result = enqueuer
        .enqueue_unique("send_email", &TestPayload { user_id: "u1".into() })
        .await
        .unwrap();

    assert!(matches!(result, EnqueueResult::Created(_)));
}

#[tokio::test]
async fn enqueue_unique_detects_duplicate() {
    let (enqueuer, _pool) = setup().await;
    let payload = TestPayload { user_id: "u1".into() };

    let first = enqueuer.enqueue_unique("send_email", &payload).await.unwrap();
    let second = enqueuer.enqueue_unique("send_email", &payload).await.unwrap();

    assert!(matches!(first, EnqueueResult::Created(_)));
    assert!(matches!(second, EnqueueResult::Duplicate(_)));
}

#[tokio::test]
async fn enqueue_unique_allows_different_payload() {
    let (enqueuer, _pool) = setup().await;

    let r1 = enqueuer
        .enqueue_unique("send_email", &TestPayload { user_id: "u1".into() })
        .await
        .unwrap();
    let r2 = enqueuer
        .enqueue_unique("send_email", &TestPayload { user_id: "u2".into() })
        .await
        .unwrap();

    assert!(matches!(r1, EnqueueResult::Created(_)));
    assert!(matches!(r2, EnqueueResult::Created(_)));
}

#[tokio::test]
async fn cancel_pending_job_succeeds() {
    let (enqueuer, _pool) = setup().await;
    let id = enqueuer
        .enqueue("test", &serde_json::json!({}))
        .await
        .unwrap();

    let cancelled = enqueuer.cancel(&id).await.unwrap();
    assert!(cancelled);
}

#[tokio::test]
async fn cancel_nonexistent_job_returns_false() {
    let (enqueuer, _pool) = setup().await;
    let cancelled = enqueuer.cancel("nonexistent").await.unwrap();
    assert!(!cancelled);
}

#[tokio::test]
async fn cancel_already_cancelled_returns_false() {
    let (enqueuer, _pool) = setup().await;
    let id = enqueuer
        .enqueue("test", &serde_json::json!({}))
        .await
        .unwrap();

    enqueuer.cancel(&id).await.unwrap();
    let second = enqueuer.cancel(&id).await.unwrap();
    assert!(!second);
}
```

- [ ] **Step 4: Verify**

Run: `cargo test --test job_enqueuer_test`
Expected: 9 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/job/enqueuer.rs src/job/mod.rs tests/job_enqueuer_test.rs
git commit -m "feat(job): add Enqueuer with enqueue, enqueue_at, enqueue_unique, cancel"
```

---

### Task 9: Worker — type-erased handler storage and builder

**Files:**
- Create: `src/job/worker.rs`
- Modify: `src/job/mod.rs`

This task creates the Worker/WorkerBuilder structures and handler registration. Polling and execution come in the next task.

- [ ] **Step 1: Create job/worker.rs with builder and type-erased handler storage**

Create `src/job/worker.rs`:

```rust
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::db::{InnerPool, Writer};
use crate::error::{Error, Result};
use crate::service::{Registry, RegistrySnapshot};

use super::config::{JobConfig, QueueConfig};
use super::context::JobContext;
use super::handler::JobHandler;
use super::meta::Meta;

pub struct JobOptions {
    pub max_attempts: u32,
    pub timeout_secs: u64,
}

impl Default for JobOptions {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            timeout_secs: 300,
        }
    }
}

type ErasedHandler = Arc<
    dyn Fn(JobContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync,
>;

struct HandlerEntry {
    handler: ErasedHandler,
    options: JobOptions,
}

pub struct WorkerBuilder {
    config: JobConfig,
    registry: Arc<RegistrySnapshot>,
    writer: InnerPool,
    handlers: HashMap<String, HandlerEntry>,
}

impl WorkerBuilder {
    pub fn register<H, Args>(mut self, name: &str, handler: H) -> Self
    where
        H: JobHandler<Args> + Send + Sync,
    {
        self.register_inner(name, handler, JobOptions::default());
        self
    }

    pub fn register_with<H, Args>(
        mut self,
        name: &str,
        handler: H,
        options: JobOptions,
    ) -> Self
    where
        H: JobHandler<Args> + Send + Sync,
    {
        self.register_inner(name, handler, options);
        self
    }

    fn register_inner<H, Args>(&mut self, name: &str, handler: H, options: JobOptions)
    where
        H: JobHandler<Args> + Send + Sync,
    {
        let handler = Arc::new(move |ctx: JobContext| -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
            let h = handler.clone();
            Box::pin(async move { h.call(ctx).await })
        }) as ErasedHandler;

        self.handlers.insert(
            name.to_string(),
            HandlerEntry { handler, options },
        );
    }

    pub async fn start(self) -> Worker {
        let cancel = CancellationToken::new();
        let handlers = Arc::new(self.handlers);
        let handler_names: Vec<String> = handlers.keys().cloned().collect();

        // Build per-queue semaphores
        let queue_semaphores: Vec<(QueueConfig, Arc<Semaphore>)> = self
            .config
            .queues
            .iter()
            .map(|q| (q.clone(), Arc::new(Semaphore::new(q.concurrency as usize))))
            .collect();

        // Spawn poll loop
        let poll_handle = tokio::spawn(poll_loop(
            self.writer.clone(),
            self.registry.clone(),
            handlers.clone(),
            handler_names,
            queue_semaphores,
            self.config.poll_interval_secs,
            cancel.clone(),
        ));

        // Spawn stale reaper
        let reaper_handle = tokio::spawn(reaper_loop(
            self.writer.clone(),
            self.config.stale_threshold_secs,
            self.config.stale_reaper_interval_secs,
            cancel.clone(),
        ));

        // Spawn cleanup (if configured)
        let cleanup_handle = if let Some(ref cleanup) = self.config.cleanup {
            Some(tokio::spawn(cleanup_loop(
                self.writer.clone(),
                cleanup.interval_secs,
                cleanup.retention_secs,
                cancel.clone(),
            )))
        } else {
            None
        };

        Worker {
            cancel,
            poll_handle,
            reaper_handle,
            cleanup_handle,
            drain_timeout: Duration::from_secs(self.config.drain_timeout_secs),
        }
    }
}

pub struct Worker {
    cancel: CancellationToken,
    poll_handle: JoinHandle<()>,
    reaper_handle: JoinHandle<()>,
    cleanup_handle: Option<JoinHandle<()>>,
    drain_timeout: Duration,
}

impl Worker {
    pub fn builder(config: &JobConfig, registry: &Registry) -> WorkerBuilder {
        let snapshot = registry.snapshot();
        let writer = snapshot
            .get::<crate::db::WritePool>()
            .expect("WritePool must be registered before building Worker");

        WorkerBuilder {
            config: config.clone(),
            registry: snapshot,
            writer: writer.write_pool().clone(),
            handlers: HashMap::new(),
        }
    }
}

impl crate::runtime::Task for Worker {
    async fn shutdown(self) -> Result<()> {
        self.cancel.cancel();

        // Wait for poll loop, reaper, cleanup to stop
        let _ = self.poll_handle.await;
        let _ = self.reaper_handle.await;
        if let Some(h) = self.cleanup_handle {
            let _ = h.await;
        }

        // drain_timeout: in-flight jobs have their own JoinHandles inside poll_loop
        // The poll loop tracks them and waits on shutdown.
        // For now, we rely on CancellationToken propagation.

        Ok(())
    }
}

// --- Background loops (stubs for now, implemented in next tasks) ---

async fn poll_loop(
    writer: InnerPool,
    registry: Arc<RegistrySnapshot>,
    handlers: Arc<HashMap<String, HandlerEntry>>,
    handler_names: Vec<String>,
    queue_semaphores: Vec<(QueueConfig, Arc<Semaphore>)>,
    poll_interval_secs: u64,
    cancel: CancellationToken,
) {
    use chrono::Utc;
    use tokio::time::{sleep, timeout};

    let poll_interval = Duration::from_secs(poll_interval_secs);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = sleep(poll_interval) => {
                let now = Utc::now();
                let now_str = now.to_rfc3339();

                for (queue_config, semaphore) in &queue_semaphores {
                    let slots = semaphore.available_permits();
                    if slots == 0 {
                        continue;
                    }

                    // Build dynamic IN clause
                    let placeholders: String = handler_names
                        .iter()
                        .map(|_| "?")
                        .collect::<Vec<_>>()
                        .join(", ");

                    let claim_sql = format!(
                        "UPDATE modo_jobs SET status = 'running', attempt = attempt + 1, \
                         started_at = ?, updated_at = ? \
                         WHERE id IN (\
                             SELECT id FROM modo_jobs \
                             WHERE status = 'pending' AND run_at <= ? \
                             AND queue = ? AND name IN ({placeholders}) \
                             ORDER BY run_at ASC LIMIT ?\
                         ) RETURNING id, name, queue, payload, attempt"
                    );

                    let mut query = sqlx::query_as::<_, (String, String, String, String, i32)>(&claim_sql)
                        .bind(&now_str)
                        .bind(&now_str)
                        .bind(&now_str)
                        .bind(&queue_config.name);

                    for name in &handler_names {
                        query = query.bind(name);
                    }
                    query = query.bind(slots as i32);

                    let claimed = match query.fetch_all(&writer).await {
                        Ok(rows) => rows,
                        Err(e) => {
                            tracing::error!(error = %e, queue = %queue_config.name, "failed to claim jobs");
                            continue;
                        }
                    };

                    for (job_id, job_name, job_queue, payload, attempt) in claimed {
                        let Some(entry) = handlers.get(&job_name) else {
                            tracing::warn!(job_name = %job_name, "no handler registered");
                            continue;
                        };

                        let permit = match semaphore.clone().acquire_owned().await {
                            Ok(p) => p,
                            Err(_) => break, // semaphore closed
                        };

                        let handler = entry.handler.clone();
                        let max_attempts = entry.options.max_attempts;
                        let timeout_secs = entry.options.timeout_secs;
                        let registry = registry.clone();
                        let writer = writer.clone();

                        let deadline = tokio::time::Instant::now()
                            + Duration::from_secs(timeout_secs);

                        let meta = Meta {
                            id: job_id.clone(),
                            name: job_name.clone(),
                            queue: job_queue,
                            attempt: attempt as u32,
                            max_attempts,
                            deadline: Some(deadline),
                        };

                        let ctx = JobContext {
                            registry,
                            payload,
                            meta,
                        };

                        tokio::spawn(async move {
                            let result = timeout(
                                Duration::from_secs(timeout_secs),
                                (handler)(ctx),
                            )
                            .await;

                            let now_str = Utc::now().to_rfc3339();

                            match result {
                                Ok(Ok(())) => {
                                    // Mark completed
                                    let _ = sqlx::query(
                                        "UPDATE modo_jobs SET status = 'completed', \
                                         completed_at = ?, updated_at = ? WHERE id = ?",
                                    )
                                    .bind(&now_str)
                                    .bind(&now_str)
                                    .bind(&job_id)
                                    .execute(&writer)
                                    .await;

                                    tracing::info!(job_id = %job_id, job_name = %job_name, "job completed");
                                }
                                Ok(Err(e)) | Err(_) => {
                                    let error_msg = match &result {
                                        Ok(Err(e)) => format!("{e}"),
                                        Err(_) => "timeout".to_string(),
                                        _ => unreachable!(),
                                    };

                                    if (attempt as u32) >= max_attempts {
                                        // Mark dead
                                        let _ = sqlx::query(
                                            "UPDATE modo_jobs SET status = 'dead', \
                                             failed_at = ?, error_message = ?, updated_at = ? WHERE id = ?",
                                        )
                                        .bind(&now_str)
                                        .bind(&error_msg)
                                        .bind(&now_str)
                                        .bind(&job_id)
                                        .execute(&writer)
                                        .await;

                                        tracing::error!(
                                            job_id = %job_id,
                                            job_name = %job_name,
                                            attempt = attempt,
                                            error = %error_msg,
                                            "job dead after max attempts"
                                        );
                                    } else {
                                        // Reschedule with backoff
                                        let delay_secs = std::cmp::min(
                                            5u64 * 2u64.pow(attempt as u32 - 1),
                                            3600,
                                        );
                                        let retry_at = (Utc::now()
                                            + chrono::Duration::seconds(delay_secs as i64))
                                            .to_rfc3339();

                                        let _ = sqlx::query(
                                            "UPDATE modo_jobs SET status = 'pending', \
                                             run_at = ?, started_at = NULL, \
                                             failed_at = ?, error_message = ?, updated_at = ? WHERE id = ?",
                                        )
                                        .bind(&retry_at)
                                        .bind(&now_str)
                                        .bind(&error_msg)
                                        .bind(&now_str)
                                        .bind(&job_id)
                                        .execute(&writer)
                                        .await;

                                        tracing::warn!(
                                            job_id = %job_id,
                                            job_name = %job_name,
                                            attempt = attempt,
                                            retry_in_secs = delay_secs,
                                            error = %error_msg,
                                            "job failed, rescheduled"
                                        );
                                    }
                                }
                            }

                            drop(permit);
                        });
                    }
                }
            }
        }
    }
}

async fn reaper_loop(
    writer: InnerPool,
    stale_threshold_secs: u64,
    interval_secs: u64,
    cancel: CancellationToken,
) {
    use chrono::Utc;

    let interval = Duration::from_secs(interval_secs);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(interval) => {
                let threshold =
                    (Utc::now() - chrono::Duration::seconds(stale_threshold_secs as i64))
                        .to_rfc3339();
                let now_str = Utc::now().to_rfc3339();

                match sqlx::query(
                    "UPDATE modo_jobs SET status = 'pending', started_at = NULL, updated_at = ? \
                     WHERE status = 'running' AND started_at < ?",
                )
                .bind(&now_str)
                .bind(&threshold)
                .execute(&writer)
                .await
                {
                    Ok(result) if result.rows_affected() > 0 => {
                        tracing::info!(
                            count = result.rows_affected(),
                            "reaped stale jobs"
                        );
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "stale reaper failed");
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn cleanup_loop(
    writer: InnerPool,
    interval_secs: u64,
    retention_secs: u64,
    cancel: CancellationToken,
) {
    use chrono::Utc;

    let interval = Duration::from_secs(interval_secs);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(interval) => {
                let threshold =
                    (Utc::now() - chrono::Duration::seconds(retention_secs as i64)).to_rfc3339();

                match sqlx::query(
                    "DELETE FROM modo_jobs \
                     WHERE status IN ('completed', 'dead', 'cancelled') AND updated_at < ?",
                )
                .bind(&threshold)
                .execute(&writer)
                .await
                {
                    Ok(result) if result.rows_affected() > 0 => {
                        tracing::info!(
                            count = result.rows_affected(),
                            "cleaned up terminal jobs"
                        );
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "job cleanup failed");
                    }
                    _ => {}
                }
            }
        }
    }
}
```

- [ ] **Step 2: Update job/mod.rs**

Add to `src/job/mod.rs`:

```rust
mod worker;

pub use worker::{JobOptions, Worker, WorkerBuilder};
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors.

- [ ] **Step 4: Commit**

```bash
git add src/job/worker.rs src/job/mod.rs
git commit -m "feat(job): add Worker with poll loop, stale reaper, and cleanup"
```

---

### Task 10: Worker integration tests

**Files:**
- Create: `tests/job_worker_test.rs`

- [ ] **Step 1: Write worker integration tests**

Create `tests/job_worker_test.rs`:

```rust
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use modo::db;
use modo::error::Result;
use modo::job::{self, Enqueuer, JobOptions, Payload, Worker};
use modo::service::Registry;

const CREATE_TABLE: &str = "
CREATE TABLE modo_jobs (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    queue         TEXT NOT NULL DEFAULT 'default',
    payload       TEXT NOT NULL DEFAULT '{}',
    payload_hash  TEXT,
    status        TEXT NOT NULL DEFAULT 'pending',
    attempt       INTEGER NOT NULL DEFAULT 0,
    run_at        TEXT NOT NULL,
    started_at    TEXT,
    completed_at  TEXT,
    failed_at     TEXT,
    error_message TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
)";

async fn setup() -> (Registry, db::Pool) {
    let config = db::SqliteConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let pool = db::connect(&config).await.unwrap();
    sqlx::query(CREATE_TABLE).execute(&*pool).await.unwrap();

    let mut registry = Registry::new();
    registry.add(pool.clone());
    registry.add(Enqueuer::new(&pool));
    (registry, pool)
}

fn fast_config() -> job::JobConfig {
    job::JobConfig {
        poll_interval_secs: 0, // instant poll for tests
        stale_threshold_secs: 2,
        stale_reaper_interval_secs: 1,
        drain_timeout_secs: 5,
        queues: vec![job::QueueConfig {
            name: "default".to_string(),
            concurrency: 2,
        }],
        cleanup: None, // disable cleanup in tests
    }
}

// A simple handler that increments a counter
async fn counting_handler(
    _payload: Payload<serde_json::Value>,
    modo::Service(counter): modo::Service<Arc<AtomicU32>>,
) -> Result<()> {
    counter.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

// A handler that always fails
async fn failing_handler(_payload: Payload<serde_json::Value>) -> Result<()> {
    Err(modo::Error::internal("intentional failure"))
}

#[tokio::test]
async fn worker_processes_enqueued_job() {
    let (mut registry, pool) = setup().await;
    let counter = Arc::new(AtomicU32::new(0));
    registry.add(counter.clone());

    let enqueuer = Enqueuer::new(&pool);
    enqueuer.enqueue("count", &serde_json::json!({})).await.unwrap();

    let worker = Worker::builder(&fast_config(), &registry)
        .register("count", counting_handler)
        .start()
        .await;

    // Give the worker time to poll and execute
    tokio::time::sleep(Duration::from_millis(500)).await;

    modo::runtime::Task::shutdown(worker).await.unwrap();

    assert_eq!(counter.load(Ordering::SeqCst), 1);

    // Verify job is marked completed
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM modo_jobs LIMIT 1")
            .fetch_one(&*pool)
            .await
            .unwrap();
    assert_eq!(status, "completed");
}

#[tokio::test]
async fn worker_retries_failed_job() {
    let (registry, pool) = setup().await;

    let enqueuer = Enqueuer::new(&pool);
    enqueuer.enqueue("fail", &serde_json::json!({})).await.unwrap();

    let worker = Worker::builder(&fast_config(), &registry)
        .register_with("fail", failing_handler, JobOptions {
            max_attempts: 2,
            timeout_secs: 5,
        })
        .start()
        .await;

    // Wait for initial attempt + retry
    tokio::time::sleep(Duration::from_secs(2)).await;

    modo::runtime::Task::shutdown(worker).await.unwrap();

    // After 2 attempts, job should be dead
    let (status, attempt): (String, i32) =
        sqlx::query_as("SELECT status, attempt FROM modo_jobs LIMIT 1")
            .fetch_one(&*pool)
            .await
            .unwrap();
    assert_eq!(status, "dead");
    assert_eq!(attempt, 2);
}

#[tokio::test]
async fn worker_ignores_unregistered_job_names() {
    let (registry, pool) = setup().await;

    let enqueuer = Enqueuer::new(&pool);
    enqueuer.enqueue("unknown_job", &serde_json::json!({})).await.unwrap();

    let worker = Worker::builder(&fast_config(), &registry)
        .register("other_job", counting_handler)
        .start()
        .await;

    tokio::time::sleep(Duration::from_millis(500)).await;

    modo::runtime::Task::shutdown(worker).await.unwrap();

    // Job should still be pending — worker doesn't know about it
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM modo_jobs LIMIT 1")
            .fetch_one(&*pool)
            .await
            .unwrap();
    assert_eq!(status, "pending");
}

#[tokio::test]
async fn worker_shutdown_is_clean() {
    let (registry, _pool) = setup().await;

    let worker = Worker::builder(&fast_config(), &registry)
        .register("noop", |_p: Payload<serde_json::Value>| async { Ok(()) })
        .start()
        .await;

    // Immediately shut down — should not hang
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        modo::runtime::Task::shutdown(worker),
    )
    .await;

    assert!(result.is_ok());
}
```

- [ ] **Step 2: Verify**

Run: `cargo test --test job_worker_test`
Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add tests/job_worker_test.rs
git commit -m "test(job): add Worker integration tests for execution, retry, and shutdown"
```

---

### Task 11: Cron meta and schedule parsing

**Files:**
- Create: `src/cron/mod.rs`
- Create: `src/cron/meta.rs`
- Create: `src/cron/schedule.rs`
- Modify: `src/lib.rs`
- Create: `tests/cron_schedule_test.rs`

- [ ] **Step 1: Create cron/mod.rs skeleton**

Create `src/cron/mod.rs`:

```rust
mod meta;
mod schedule;

pub use meta::Meta;
pub(crate) use schedule::Schedule;
```

- [ ] **Step 2: Create cron/meta.rs**

Create `src/cron/meta.rs`:

```rust
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct Meta {
    pub name: String,
    pub deadline: Option<tokio::time::Instant>,
    pub tick: DateTime<Utc>,
}
```

- [ ] **Step 3: Create cron/schedule.rs**

Create `src/cron/schedule.rs`:

```rust
use std::time::Duration;

use chrono::{DateTime, Utc};

pub(crate) enum Schedule {
    Cron(croner::Cron),
    Interval(Duration),
}

impl Schedule {
    pub(crate) fn parse(input: &str) -> Self {
        let trimmed = input.trim();

        // Named aliases
        match trimmed {
            "@yearly" | "@annually" => return Self::parse_cron("0 0 0 1 1 *"),
            "@monthly" => return Self::parse_cron("0 0 0 1 * *"),
            "@weekly" => return Self::parse_cron("0 0 0 * * 0"),
            "@daily" | "@midnight" => return Self::parse_cron("0 0 0 * * *"),
            "@hourly" => return Self::parse_cron("0 0 * * * *"),
            _ => {}
        }

        // @every duration
        if let Some(dur_str) = trimmed.strip_prefix("@every ") {
            let duration = parse_duration(dur_str.trim());
            return Self::Interval(duration);
        }

        // Standard cron expression
        Self::parse_cron(trimmed)
    }

    fn parse_cron(expr: &str) -> Self {
        let cron = croner::Cron::new(expr)
            .parse()
            .unwrap_or_else(|e| panic!("invalid cron expression '{expr}': {e}"));
        Self::Cron(cron)
    }

    pub(crate) fn next_tick(&self, from: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            Self::Cron(cron) => cron
                .find_next_occurrence(&from, false)
                .expect("cron expression has no future occurrence"),
            Self::Interval(dur) => from + chrono::Duration::from_std(*dur).unwrap(),
        }
    }
}

fn parse_duration(s: &str) -> Duration {
    let mut total_secs: u64 = 0;
    let mut current_num = String::new();
    let mut found_any = false;

    for ch in s.chars() {
        match ch {
            '0'..='9' => current_num.push(ch),
            'h' => {
                let n: u64 = current_num
                    .parse()
                    .unwrap_or_else(|_| panic!("invalid duration '{s}': bad number before 'h'"));
                total_secs += n * 3600;
                current_num.clear();
                found_any = true;
            }
            'm' => {
                let n: u64 = current_num
                    .parse()
                    .unwrap_or_else(|_| panic!("invalid duration '{s}': bad number before 'm'"));
                total_secs += n * 60;
                current_num.clear();
                found_any = true;
            }
            's' => {
                let n: u64 = current_num
                    .parse()
                    .unwrap_or_else(|_| panic!("invalid duration '{s}': bad number before 's'"));
                total_secs += n;
                current_num.clear();
                found_any = true;
            }
            _ => panic!("invalid duration '{s}': unexpected character '{ch}'"),
        }
    }

    if !current_num.is_empty() {
        panic!("invalid duration '{s}': trailing number without unit (use h, m, or s)");
    }

    if !found_any {
        panic!("invalid duration '{s}': no duration components found");
    }

    Duration::from_secs(total_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_hours() {
        assert_eq!(parse_duration("2h"), Duration::from_secs(7200));
    }

    #[test]
    fn parse_duration_minutes() {
        assert_eq!(parse_duration("15m"), Duration::from_secs(900));
    }

    #[test]
    fn parse_duration_seconds() {
        assert_eq!(parse_duration("30s"), Duration::from_secs(30));
    }

    #[test]
    fn parse_duration_combined() {
        assert_eq!(parse_duration("1h30m"), Duration::from_secs(5400));
        assert_eq!(parse_duration("2h30m15s"), Duration::from_secs(9015));
    }

    #[test]
    #[should_panic(expected = "invalid duration")]
    fn parse_duration_rejects_days() {
        parse_duration("1d");
    }

    #[test]
    #[should_panic(expected = "invalid duration")]
    fn parse_duration_rejects_ms() {
        parse_duration("500ms");
    }

    #[test]
    #[should_panic(expected = "trailing number without unit")]
    fn parse_duration_rejects_bare_number() {
        parse_duration("30");
    }

    #[test]
    #[should_panic(expected = "no duration components")]
    fn parse_duration_rejects_empty() {
        parse_duration("");
    }
}
```

- [ ] **Step 4: Add cron module to lib.rs**

In `src/lib.rs`, add:

```rust
pub mod cron;
```

- [ ] **Step 5: Write schedule integration test**

Create `tests/cron_schedule_test.rs`:

```rust
use chrono::Utc;

// We can't construct Schedule directly (pub(crate)), but we can test
// via public Scheduler API later. For now, test the parse_duration via
// the inline unit tests and verify the module compiles.

#[test]
fn cron_module_compiles() {
    // Verify cron::Meta is accessible
    let _meta = modo::cron::Meta {
        name: "test".to_string(),
        deadline: None,
        tick: Utc::now(),
    };
}
```

- [ ] **Step 6: Verify**

Run: `cargo test --lib cron::schedule && cargo test --test cron_schedule_test`
Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/cron/ src/lib.rs tests/cron_schedule_test.rs
git commit -m "feat(cron): add Meta, schedule parsing with @every, aliases, and standard cron"
```

---

### Task 12: CronContext, FromCronContext, and CronHandler

**Files:**
- Create: `src/cron/context.rs`
- Create: `src/cron/handler.rs`
- Modify: `src/cron/mod.rs`
- Create: `tests/cron_handler_test.rs`

- [ ] **Step 1: Create cron/context.rs**

Create `src/cron/context.rs`:

```rust
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::extractor::Service;
use crate::service::RegistrySnapshot;

use super::meta::Meta;

pub(crate) struct CronContext {
    pub(crate) registry: Arc<RegistrySnapshot>,
    pub(crate) meta: Meta,
}

pub trait FromCronContext: Sized {
    fn from_cron_context(ctx: &CronContext) -> Result<Self>;
}

impl<T: Send + Sync + 'static> FromCronContext for Service<T> {
    fn from_cron_context(ctx: &CronContext) -> Result<Self> {
        ctx.registry.get::<T>().map(Service).ok_or_else(|| {
            Error::internal(format!(
                "service not found in registry: {}",
                std::any::type_name::<T>()
            ))
        })
    }
}

impl FromCronContext for Meta {
    fn from_cron_context(ctx: &CronContext) -> Result<Self> {
        Ok(ctx.meta.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::{Any, TypeId};
    use std::collections::HashMap;

    fn test_context() -> CronContext {
        let mut services: HashMap<TypeId, Arc<dyn Any + Send + Sync>> = HashMap::new();
        services.insert(TypeId::of::<u32>(), Arc::new(42u32));
        let snapshot = Arc::new(RegistrySnapshot::new(services));

        CronContext {
            registry: snapshot,
            meta: Meta {
                name: "test_job".to_string(),
                deadline: None,
                tick: chrono::Utc::now(),
            },
        }
    }

    #[test]
    fn service_extractor_finds_registered() {
        let ctx = test_context();
        let svc = Service::<u32>::from_cron_context(&ctx).unwrap();
        assert_eq!(*svc.0, 42);
    }

    #[test]
    fn service_extractor_fails_for_missing() {
        let ctx = test_context();
        let result = Service::<String>::from_cron_context(&ctx);
        assert!(result.is_err());
    }

    #[test]
    fn meta_extractor_returns_meta() {
        let ctx = test_context();
        let meta = Meta::from_cron_context(&ctx).unwrap();
        assert_eq!(meta.name, "test_job");
    }
}
```

- [ ] **Step 2: Create cron/handler.rs**

Create `src/cron/handler.rs`:

```rust
use std::future::Future;

use crate::error::Result;

use super::context::{CronContext, FromCronContext};

pub trait CronHandler<Args>: Clone + Send + 'static {
    fn call(self, ctx: CronContext) -> impl Future<Output = Result<()>> + Send;
}

// 0 args
impl<F, Fut> CronHandler<()> for F
where
    F: FnOnce() -> Fut + Clone + Send + 'static,
    Fut: Future<Output = Result<()>> + Send,
{
    async fn call(self, _ctx: CronContext) -> Result<()> {
        (self)().await
    }
}

macro_rules! impl_cron_handler {
    ($($T:ident),+) => {
        impl<F, Fut, $($T),+> CronHandler<($($T,)+)> for F
        where
            F: FnOnce($($T),+) -> Fut + Clone + Send + 'static,
            Fut: Future<Output = Result<()>> + Send,
            $($T: FromCronContext,)+
        {
            async fn call(self, ctx: CronContext) -> Result<()> {
                $(let $T = $T::from_cron_context(&ctx)?;)+
                (self)($($T),+).await
            }
        }
    };
}

impl_cron_handler!(T1);
impl_cron_handler!(T1, T2);
impl_cron_handler!(T1, T2, T3);
impl_cron_handler!(T1, T2, T3, T4);
impl_cron_handler!(T1, T2, T3, T4, T5);
impl_cron_handler!(T1, T2, T3, T4, T5, T6);
impl_cron_handler!(T1, T2, T3, T4, T5, T6, T7);
impl_cron_handler!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_cron_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_cron_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_cron_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_cron_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
```

- [ ] **Step 3: Update cron/mod.rs**

Update `src/cron/mod.rs`:

```rust
mod context;
mod handler;
mod meta;
mod schedule;

pub use context::FromCronContext;
pub(crate) use context::CronContext;
pub use handler::CronHandler;
pub use meta::Meta;
pub(crate) use schedule::Schedule;
```

- [ ] **Step 4: Write handler compile-time test**

Create `tests/cron_handler_test.rs`:

```rust
use modo::cron::{CronHandler, Meta};
use modo::error::Result;
use modo::Service;

async fn noop() -> Result<()> {
    Ok(())
}

async fn with_service(_svc: Service<String>) -> Result<()> {
    Ok(())
}

async fn with_meta(_meta: Meta) -> Result<()> {
    Ok(())
}

async fn with_both(_svc: Service<String>, _meta: Meta) -> Result<()> {
    Ok(())
}

fn assert_cron_handler<H: CronHandler<Args>, Args>(_h: H) {}

#[test]
fn zero_arg_handler_satisfies_trait() {
    assert_cron_handler(noop);
}

#[test]
fn service_handler_satisfies_trait() {
    assert_cron_handler(with_service);
}

#[test]
fn meta_handler_satisfies_trait() {
    assert_cron_handler(with_meta);
}

#[test]
fn multi_arg_handler_satisfies_trait() {
    assert_cron_handler(with_both);
}
```

- [ ] **Step 5: Verify**

Run: `cargo test --lib cron::context && cargo test --test cron_handler_test`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/cron/context.rs src/cron/handler.rs src/cron/mod.rs tests/cron_handler_test.rs
git commit -m "feat(cron): add CronContext, FromCronContext, and CronHandler trait"
```

---

### Task 13: Cron Scheduler

**Files:**
- Create: `src/cron/scheduler.rs`
- Modify: `src/cron/mod.rs`
- Create: `tests/cron_scheduler_test.rs`

- [ ] **Step 1: Create cron/scheduler.rs**

Create `src/cron/scheduler.rs`:

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::error::Result;
use crate::service::{Registry, RegistrySnapshot};

use super::context::CronContext;
use super::handler::CronHandler;
use super::meta::Meta;
use super::schedule::Schedule;

pub struct CronOptions {
    pub timeout_secs: u64,
}

impl Default for CronOptions {
    fn default() -> Self {
        Self { timeout_secs: 300 }
    }
}

type ErasedCronHandler = Arc<
    dyn Fn(CronContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync,
>;

struct CronEntry {
    name: String,
    schedule: Schedule,
    handler: ErasedCronHandler,
    timeout_secs: u64,
}

pub struct SchedulerBuilder {
    registry: Arc<RegistrySnapshot>,
    entries: Vec<CronEntry>,
}

impl SchedulerBuilder {
    pub fn job<H, Args>(self, schedule: &str, handler: H) -> Self
    where
        H: CronHandler<Args> + Send + Sync,
    {
        self.job_with(schedule, handler, CronOptions::default())
    }

    pub fn job_with<H, Args>(
        mut self,
        schedule: &str,
        handler: H,
        options: CronOptions,
    ) -> Self
    where
        H: CronHandler<Args> + Send + Sync,
    {
        let name = std::any::type_name_of_val(&handler).to_string();
        let parsed = Schedule::parse(schedule);

        let erased: ErasedCronHandler = Arc::new(move |ctx: CronContext| -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
            let h = handler.clone();
            Box::pin(async move { h.call(ctx).await })
        });

        self.entries.push(CronEntry {
            name,
            schedule: parsed,
            handler: erased,
            timeout_secs: options.timeout_secs,
        });
        self
    }

    pub async fn start(self) -> Scheduler {
        let cancel = CancellationToken::new();
        let mut handles = Vec::new();

        for entry in self.entries {
            let handle = tokio::spawn(cron_job_loop(
                entry.name,
                entry.schedule,
                entry.handler,
                entry.timeout_secs,
                self.registry.clone(),
                cancel.clone(),
            ));
            handles.push(handle);
        }

        Scheduler { cancel, handles }
    }
}

pub struct Scheduler {
    cancel: CancellationToken,
    handles: Vec<JoinHandle<()>>,
}

impl Scheduler {
    pub fn builder(registry: &Registry) -> SchedulerBuilder {
        SchedulerBuilder {
            registry: registry.snapshot(),
            entries: Vec::new(),
        }
    }
}

impl crate::runtime::Task for Scheduler {
    async fn shutdown(self) -> Result<()> {
        self.cancel.cancel();
        for handle in self.handles {
            let _ = handle.await;
        }
        Ok(())
    }
}

async fn cron_job_loop(
    name: String,
    schedule: Schedule,
    handler: ErasedCronHandler,
    timeout_secs: u64,
    registry: Arc<RegistrySnapshot>,
    cancel: CancellationToken,
) {
    let running = Arc::new(AtomicBool::new(false));
    let timeout_dur = Duration::from_secs(timeout_secs);

    let mut next_tick = schedule.next_tick(Utc::now());

    loop {
        let sleep_duration = (next_tick - Utc::now())
            .to_std()
            .unwrap_or(Duration::ZERO);

        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(sleep_duration) => {
                // Skip if previous run still going
                if running.load(Ordering::SeqCst) {
                    tracing::warn!(cron_job = %name, "skipping tick, previous run still active");
                    next_tick = schedule.next_tick(Utc::now());
                    continue;
                }

                running.store(true, Ordering::SeqCst);

                let deadline = tokio::time::Instant::now() + timeout_dur;

                let ctx = CronContext {
                    registry: registry.clone(),
                    meta: Meta {
                        name: name.clone(),
                        deadline: Some(deadline),
                        tick: next_tick,
                    },
                };

                let result = tokio::time::timeout(timeout_dur, (handler)(ctx)).await;

                match result {
                    Ok(Ok(())) => {
                        tracing::debug!(cron_job = %name, "completed");
                    }
                    Ok(Err(e)) => {
                        tracing::error!(cron_job = %name, error = %e, "failed");
                    }
                    Err(_) => {
                        tracing::error!(cron_job = %name, "timed out");
                    }
                }

                running.store(false, Ordering::SeqCst);
                next_tick = schedule.next_tick(Utc::now());
            }
        }
    }
}
```

- [ ] **Step 2: Update cron/mod.rs**

Update `src/cron/mod.rs`:

```rust
mod context;
mod handler;
mod meta;
mod schedule;
mod scheduler;

pub use context::FromCronContext;
pub(crate) use context::CronContext;
pub use handler::CronHandler;
pub use meta::Meta;
pub use scheduler::{CronOptions, Scheduler, SchedulerBuilder};
pub(crate) use schedule::Schedule;
```

- [ ] **Step 3: Write scheduler integration tests**

Create `tests/cron_scheduler_test.rs`:

```rust
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use modo::cron::{CronOptions, Scheduler};
use modo::error::Result;
use modo::service::Registry;
use modo::Service;

async fn counting_job(
    Service(counter): Service<Arc<AtomicU32>>,
) -> Result<()> {
    counter.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

async fn slow_job(
    Service(counter): Service<Arc<AtomicU32>>,
) -> Result<()> {
    tokio::time::sleep(Duration::from_secs(10)).await;
    counter.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

#[tokio::test]
async fn scheduler_runs_job_on_interval() {
    let counter = Arc::new(AtomicU32::new(0));
    let mut registry = Registry::new();
    registry.add(counter.clone());

    let scheduler = Scheduler::builder(&registry)
        .job("@every 1s", counting_job)
        .start()
        .await;

    // Wait for 2-3 ticks
    tokio::time::sleep(Duration::from_millis(2500)).await;

    modo::runtime::Task::shutdown(scheduler).await.unwrap();

    let count = counter.load(Ordering::SeqCst);
    assert!(count >= 2, "expected at least 2 executions, got {count}");
}

#[tokio::test]
async fn scheduler_shutdown_is_clean() {
    let mut registry = Registry::new();
    registry.add(Arc::new(AtomicU32::new(0)));

    let scheduler = Scheduler::builder(&registry)
        .job("@every 1s", counting_job)
        .start()
        .await;

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        modo::runtime::Task::shutdown(scheduler),
    )
    .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn scheduler_skips_overlapping_runs() {
    let counter = Arc::new(AtomicU32::new(0));
    let mut registry = Registry::new();
    registry.add(counter.clone());

    let scheduler = Scheduler::builder(&registry)
        .job_with("@every 1s", slow_job, CronOptions { timeout_secs: 30 })
        .start()
        .await;

    // slow_job takes 10s, interval is 1s — should skip overlapping ticks
    tokio::time::sleep(Duration::from_millis(3500)).await;

    modo::runtime::Task::shutdown(scheduler).await.unwrap();

    // Only 1 execution should have started (still running when shutdown)
    let count = counter.load(Ordering::SeqCst);
    assert!(count <= 1, "expected at most 1 execution, got {count}");
}

#[tokio::test]
async fn scheduler_timeout_cancels_job() {
    let counter = Arc::new(AtomicU32::new(0));
    let mut registry = Registry::new();
    registry.add(counter.clone());

    let scheduler = Scheduler::builder(&registry)
        .job_with("@every 1s", slow_job, CronOptions { timeout_secs: 1 })
        .start()
        .await;

    // Let it timeout once
    tokio::time::sleep(Duration::from_millis(2500)).await;

    modo::runtime::Task::shutdown(scheduler).await.unwrap();

    // Counter should be 0 — the slow_job sleeps 10s but timeout is 1s
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}
```

- [ ] **Step 4: Verify**

Run: `cargo test --test cron_scheduler_test`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/cron/scheduler.rs src/cron/mod.rs tests/cron_scheduler_test.rs
git commit -m "feat(cron): add Scheduler with interval execution, overlap protection, and timeout"
```

---

### Task 14: Wire up lib.rs re-exports and update CLAUDE.md

**Files:**
- Modify: `src/lib.rs`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update lib.rs re-exports**

Ensure `src/lib.rs` has the final re-exports:

```rust
pub mod job;
pub mod cron;
```

No additional pub use needed — users access via `modo::job::*` and `modo::cron::*`.

- [ ] **Step 2: Update CLAUDE.md roadmap**

Update the Implementation Roadmap section:

```markdown
- **Plan 5 (Job + Cron):** DB-backed job queue, worker, enqueuer, in-memory cron scheduler — DONE
```

Add to Key References:

```markdown
- Job + Cron spec: `docs/superpowers/specs/2026-03-20-modo-v2-job-cron-design.md`
- Job + Cron plan: `docs/superpowers/plans/2026-03-20-modo-v2-job-cron.md`
```

Add to Gotchas:

```markdown
- `std::any::type_name_of_val` requires nightly or Rust 2024 edition — if unavailable, use `std::any::type_name::<H>()` in the CronHandler impl instead
- Worker poll loop builds dynamic SQL with `IN (?, ?, ...)` — SQLite limits to 999 bind params, so max ~900 registered handlers per worker
- Job `attempt` is incremented on claim (not on failure) — a job with `attempt=3` has been claimed 3 times regardless of outcome
- `tokio_util::sync::CancellationToken` is used for all background loop shutdown — always check `cancel.cancelled()` in `tokio::select!`
```

- [ ] **Step 3: Run full test suite**

Run: `cargo test && cargo clippy --tests -- -D warnings`
Expected: all tests pass, no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/lib.rs CLAUDE.md
git commit -m "docs: update CLAUDE.md with Plan 5 completion and new gotchas"
```
