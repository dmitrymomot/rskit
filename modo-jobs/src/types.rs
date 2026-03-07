use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Unique identifier for a job, backed by a ULID string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct JobId(String);

impl JobId {
    /// Generate a new unique job ID.
    pub fn new() -> Self {
        Self(ulid::Ulid::new().to_string())
    }

    /// View the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Create a `JobId` from an existing raw string (e.g. from DB).
    pub fn from_raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// State of a job in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Pending,
    Running,
    Completed,
    Dead,
    Cancelled,
}

impl JobState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Dead => "dead",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for JobState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for JobState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "dead" => Ok(Self::Dead),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(format!("unknown job state: {other}")),
        }
    }
}
