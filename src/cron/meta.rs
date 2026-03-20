use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct Meta {
    pub name: String,
    pub deadline: Option<tokio::time::Instant>,
    pub tick: DateTime<Utc>,
}
