use crate::error::Error;
use crate::jobs::entity;
use crate::jobs::handler::JobRegistration;
use crate::jobs::store::SqliteJobStore;
use crate::jobs::types::{JobId, NewJob};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

static GLOBAL_QUEUE: std::sync::OnceLock<JobQueue> = std::sync::OnceLock::new();

#[derive(Clone)]
pub struct JobQueue {
    pub(crate) store: Arc<SqliteJobStore>,
}

impl JobQueue {
    pub(crate) fn new(store: Arc<SqliteJobStore>) -> Self {
        Self { store }
    }

    pub(crate) fn set_global(queue: JobQueue) {
        GLOBAL_QUEUE.set(queue).ok();
    }

    /// Access the global job queue (set at startup).
    pub fn global() -> &'static JobQueue {
        GLOBAL_QUEUE.get().expect(
            "JobQueue not initialized — ensure app has a database and jobs feature is enabled",
        )
    }

    /// Enqueue a job using its registered defaults.
    pub async fn enqueue<T: Serialize>(&self, name: &str, payload: &T) -> Result<JobId, Error> {
        let reg = find_registration(name)?;
        let payload_json = serde_json::to_value(payload)
            .map_err(|e| Error::internal(format!("failed to serialize job payload: {e}")))?;

        self.store
            .enqueue(NewJob {
                name: name.to_string(),
                queue: reg.queue.to_string(),
                payload: payload_json,
                priority: 0,
                max_retries: reg.max_retries,
                run_at: Utc::now(),
                timeout_secs: reg.timeout.as_secs() as u32,
                dedupe_key: None,
                tenant_id: None,
            })
            .await
    }

    /// Enqueue a job to run at a specific time.
    pub async fn enqueue_at<T: Serialize>(
        &self,
        name: &str,
        payload: &T,
        run_at: DateTime<Utc>,
    ) -> Result<JobId, Error> {
        let reg = find_registration(name)?;
        let payload_json = serde_json::to_value(payload)
            .map_err(|e| Error::internal(format!("failed to serialize job payload: {e}")))?;

        self.store
            .enqueue(NewJob {
                name: name.to_string(),
                queue: reg.queue.to_string(),
                payload: payload_json,
                priority: 0,
                max_retries: reg.max_retries,
                run_at,
                timeout_secs: reg.timeout.as_secs() as u32,
                dedupe_key: None,
                tenant_id: None,
            })
            .await
    }

    /// Create a `JobBuilder` for full control over job parameters.
    pub fn build(&self, name: &str) -> JobBuilder {
        let reg = find_registration(name).ok();
        JobBuilder {
            store: self.store.clone(),
            name: name.to_string(),
            queue: reg
                .map(|r| r.queue.to_string())
                .unwrap_or_else(|| "default".to_string()),
            priority: 0,
            max_retries: reg.map(|r| r.max_retries).unwrap_or(3),
            timeout_secs: reg.map(|r| r.timeout.as_secs() as u32).unwrap_or(300),
            run_at: Utc::now(),
            dedupe_key: None,
            tenant_id: None,
        }
    }

    pub async fn get(&self, id: &JobId) -> Result<Option<entity::Model>, Error> {
        self.store.get(id).await
    }

    pub async fn cancel(&self, id: &JobId) -> Result<(), Error> {
        self.store.cancel(id).await
    }

    pub async fn cleanup(&self, older_than: Duration) -> Result<u64, Error> {
        self.store.cleanup(older_than).await
    }
}

fn find_registration(name: &str) -> Result<&'static JobRegistration, Error> {
    inventory::iter::<JobRegistration>
        .into_iter()
        .find(|r| r.name == name)
        .ok_or_else(|| Error::internal(format!("no job registration found for '{name}'")))
}

pub struct JobBuilder {
    store: Arc<SqliteJobStore>,
    name: String,
    queue: String,
    priority: i32,
    max_retries: u32,
    timeout_secs: u32,
    run_at: DateTime<Utc>,
    dedupe_key: Option<String>,
    tenant_id: Option<String>,
}

impl JobBuilder {
    pub fn queue(mut self, queue: &str) -> Self {
        self.queue = queue.to_string();
        self
    }

    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout_secs = timeout.as_secs() as u32;
        self
    }

    pub fn run_at(mut self, run_at: DateTime<Utc>) -> Self {
        self.run_at = run_at;
        self
    }

    pub fn dedupe_key(mut self, key: &str) -> Self {
        self.dedupe_key = Some(key.to_string());
        self
    }

    pub fn tenant_id(mut self, id: &str) -> Self {
        self.tenant_id = Some(id.to_string());
        self
    }

    pub async fn enqueue<T: Serialize>(self, payload: &T) -> Result<JobId, Error> {
        let payload_json = serde_json::to_value(payload)
            .map_err(|e| Error::internal(format!("failed to serialize job payload: {e}")))?;

        self.store
            .enqueue(NewJob {
                name: self.name,
                queue: self.queue,
                payload: payload_json,
                priority: self.priority,
                max_retries: self.max_retries,
                run_at: self.run_at,
                timeout_secs: self.timeout_secs,
                dedupe_key: self.dedupe_key,
                tenant_id: self.tenant_id,
            })
            .await
    }
}
