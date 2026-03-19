mod init;
mod sentry;

pub use init::{Config, init};
#[cfg(feature = "sentry")]
pub use sentry::SentryConfig;
pub use sentry::TracingGuard;

// Re-export tracing macros so $crate::tracing::info! works in run! macro
pub use ::tracing::{debug, error, info, trace, warn};
