use chrono::{DateTime, Utc};

/// Metadata about the current cron job execution passed to handlers.
///
/// Obtain a `Meta` value inside a handler by declaring it as a handler
/// argument — the scheduler extracts it from the [`CronContext`](super::context::CronContext)
/// automatically via [`FromCronContext`](super::context::FromCronContext).
#[derive(Debug, Clone)]
pub struct Meta {
    /// Fully qualified type name of the registered handler function.
    pub name: String,
    /// Deadline by which the handler must finish, derived from
    /// [`CronOptions::timeout_secs`](super::scheduler::CronOptions).
    /// Always `Some` for jobs started through [`SchedulerBuilder`](super::scheduler::SchedulerBuilder).
    pub deadline: Option<tokio::time::Instant>,
    /// The scheduled tick time that triggered this execution.
    pub tick: DateTime<Utc>,
}
