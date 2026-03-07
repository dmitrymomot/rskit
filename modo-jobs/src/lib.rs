pub mod config;
pub mod entity;
pub mod handler;
pub mod queue;
pub mod types;

// Public API
pub use config::{CleanupConfig, JobsConfig, QueueConfig};
pub use handler::{JobContext, JobHandler, JobHandlerDyn, JobRegistration};
pub use queue::JobQueue;
pub use types::{JobId, JobState};

// Re-export proc macros
pub use modo_jobs_macros::job;

// Re-exports for macro-generated code
pub use chrono;
pub use inventory;
pub use modo;
pub use modo_db;
pub use serde_json;
