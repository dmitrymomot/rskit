use std::fmt;

/// Job lifecycle status.
///
/// | Variant | Meaning |
/// |---|---|
/// | `Pending` | Waiting to be picked up by a worker |
/// | `Running` | Currently being executed |
/// | `Completed` | Finished successfully |
/// | `Dead` | Exhausted all retry attempts |
/// | `Cancelled` | Cancelled before execution via [`Enqueuer::cancel`](super::enqueuer::Enqueuer::cancel) |
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Waiting to be picked up by a worker.
    Pending,
    /// Currently being executed.
    Running,
    /// Finished successfully.
    Completed,
    /// Exhausted all retry attempts; will not be retried.
    Dead,
    /// Cancelled before execution.
    Cancelled,
}

impl Status {
    /// Returns the lowercase string representation of this status.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Dead => "dead",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parses a status from its lowercase string representation.
    ///
    /// Returns `None` for unknown strings.
    #[allow(clippy::should_implement_trait)]
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

    /// Returns `true` for statuses that are final: `Completed`, `Dead`, or
    /// `Cancelled`.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Dead | Self::Cancelled)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Metadata about the currently executing job, available as a handler argument.
///
/// Extract `Meta` in a handler function to inspect the job's identity and
/// retry state at runtime.
#[derive(Debug, Clone)]
pub struct Meta {
    /// Unique job ID (ULID format).
    pub id: String,
    /// Registered handler name used to identify the job type.
    pub name: String,
    /// Name of the queue this job belongs to.
    pub queue: String,
    /// Current attempt number (1-based; incremented on each execution).
    pub attempt: u32,
    /// Maximum number of attempts before the job is marked `Dead`.
    pub max_attempts: u32,
    /// Absolute deadline for this execution; `None` if no timeout is set.
    pub deadline: Option<tokio::time::Instant>,
}

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
