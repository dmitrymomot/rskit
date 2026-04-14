use std::pin::Pin;
use std::sync::Arc;

use crate::Result;

/// A health check that can verify the readiness of a service.
///
/// Implement this trait for types that can verify their own health (e.g.,
/// database pools, cache connections). The check should be fast and
/// non-destructive.
///
/// When the `db` feature is enabled, [`crate::db::Database`] implements this
/// trait automatically.
pub trait HealthCheck: Send + Sync + 'static {
    /// Run the health check.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] if the service is unhealthy or unreachable.
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}

/// Internal adapter that wraps a closure into a [`HealthCheck`].
struct FnHealthCheck<F>(F);

impl<F, Fut> HealthCheck for FnHealthCheck<F>
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin((self.0)())
    }
}

/// A collection of named health checks.
///
/// Built with a fluent API and registered in the
/// [`service::Registry`](crate::service::Registry). The readiness endpoint
/// runs all checks concurrently and reports failures.
///
/// # Example
///
/// ```
/// use modo::HealthChecks;
///
/// let checks = HealthChecks::new()
///     .check_fn("database", || async { Ok(()) })
///     .check_fn("redis", || async { Ok(()) });
/// ```
pub struct HealthChecks {
    checks: Vec<(String, Arc<dyn HealthCheck>)>,
}

impl HealthChecks {
    /// Creates an empty collection.
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// Register a named [`HealthCheck`] implementation under the given name.
    pub fn check(mut self, name: &str, c: impl HealthCheck) -> Self {
        self.checks.push((name.to_owned(), Arc::new(c)));
        self
    }

    /// Register a named health check from an async closure.
    ///
    /// The closure must return [`crate::Result<()>`].
    pub fn check_fn<F, Fut>(mut self, name: &str, f: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.checks
            .push((name.to_owned(), Arc::new(FnHealthCheck(f))));
        self
    }

    /// Returns a slice of all registered checks.
    pub(crate) fn as_slice(&self) -> &[(String, Arc<dyn HealthCheck>)] {
        &self.checks
    }
}

/// Returns an empty [`HealthChecks`] collection.
impl Default for HealthChecks {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthCheck for crate::db::Database {
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async {
            self.conn()
                .query("SELECT 1", ())
                .await
                .map_err(|e| crate::Error::internal("db health check failed").chain(e))?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn database_health_check() {
        let config = crate::db::Config {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        let db = crate::db::connect(&config).await.unwrap();
        db.check().await.unwrap();
    }

    #[tokio::test]
    async fn fn_health_check_succeeds() {
        let checks = HealthChecks::new().check_fn("ok", || async { Ok(()) });
        let (_, c) = &checks.as_slice()[0];
        c.check().await.unwrap();
    }

    #[tokio::test]
    async fn fn_health_check_fails() {
        let checks =
            HealthChecks::new().check_fn("fail", || async { Err(crate::Error::internal("down")) });
        let (_, c) = &checks.as_slice()[0];
        assert!(c.check().await.is_err());
    }

    #[tokio::test]
    async fn chaining_preserves_order() {
        let checks = HealthChecks::new()
            .check_fn("a", || async { Ok(()) })
            .check_fn("b", || async { Ok(()) })
            .check_fn("c", || async { Ok(()) });
        let names: Vec<&str> = checks.as_slice().iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn health_checks_default_is_empty() {
        let checks = HealthChecks::default();
        assert!(checks.as_slice().is_empty());
    }
}
