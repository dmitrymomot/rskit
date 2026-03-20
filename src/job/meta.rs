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
