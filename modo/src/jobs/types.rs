use crate::app::AppState;
use crate::error::Error;
use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

/// Opaque job identifier (ULID string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(String);

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl JobId {
    pub fn new() -> Self {
        Self(ulid::Ulid::new().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn from_raw(s: String) -> Self {
        Self(s)
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Job execution state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobState {
    Pending,
    Running,
    Completed,
    Failed,
    Dead,
}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => f.write_str("pending"),
            Self::Running => f.write_str("running"),
            Self::Completed => f.write_str("completed"),
            Self::Failed => f.write_str("failed"),
            Self::Dead => f.write_str("dead"),
        }
    }
}

impl FromStr for JobState {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "dead" => Ok(Self::Dead),
            _ => Err(Error::internal(format!("unknown job state: {s}"))),
        }
    }
}

/// Input for enqueuing a new job.
pub struct NewJob {
    pub name: String,
    pub queue: String,
    pub payload: serde_json::Value,
    pub priority: i32,
    pub max_retries: u32,
    pub run_at: DateTime<Utc>,
    pub timeout_secs: u32,
    pub dedupe_key: Option<String>,
    pub tenant_id: Option<String>,
}

/// Context passed to job handlers during execution.
pub struct JobContext {
    pub job_id: JobId,
    pub name: String,
    pub queue: String,
    pub attempt: u32,
    pub(crate) app_state: AppState,
    pub(crate) payload_json: serde_json::Value,
}

impl JobContext {
    /// Deserialize the job payload.
    pub fn payload<T: serde::de::DeserializeOwned>(&self) -> Result<T, Error> {
        serde_json::from_value(self.payload_json.clone())
            .map_err(|e| Error::internal(format!("failed to deserialize job payload: {e}")))
    }

    /// Get a service from the app's service registry.
    pub fn service<T: Send + Sync + 'static>(&self) -> Result<Arc<T>, Error> {
        self.app_state.services.get::<T>().ok_or_else(|| {
            Error::internal(format!(
                "Service not registered: {}",
                std::any::type_name::<T>()
            ))
        })
    }

    /// Get the database connection.
    pub fn db(&self) -> Result<&DatabaseConnection, Error> {
        self.app_state
            .db
            .as_ref()
            .ok_or_else(|| Error::internal("Database not configured"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_id_generates_unique() {
        let a = JobId::new();
        let b = JobId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn job_id_is_26_char_ulid() {
        let id = JobId::new();
        assert_eq!(id.as_str().len(), 26);
    }

    #[test]
    fn job_state_display_roundtrip() {
        let states = [
            JobState::Pending,
            JobState::Running,
            JobState::Completed,
            JobState::Failed,
            JobState::Dead,
        ];
        for state in states {
            let s = state.to_string();
            let parsed: JobState = s.parse().unwrap();
            assert_eq!(state, parsed);
        }
    }

    #[test]
    fn job_state_from_str_invalid() {
        let result = "bogus".parse::<JobState>();
        assert!(result.is_err());
    }
}
