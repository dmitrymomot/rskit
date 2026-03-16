use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

modo::ulid_id!(JobId);

/// State of a job in the queue.
///
/// Serialized as lowercase strings (`"pending"`, `"running"`, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    /// Waiting to be picked up by a worker.
    Pending,
    /// Currently executing on a worker.
    Running,
    /// Finished successfully.
    Completed,
    /// Exhausted all retry attempts without succeeding.
    Dead,
    /// Cancelled before execution.
    Cancelled,
}

impl JobState {
    /// Return the lowercase string representation of this state.
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

    /// Parse a lowercase state string.  Returns an error for unknown values.
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
