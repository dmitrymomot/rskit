use std::pin::Pin;
use std::sync::Arc;

use crate::Result;

/// A health check that can verify the readiness of a service.
///
/// Implement this trait for types that can verify their own health (e.g.,
/// database pools, cache connections). The check should be fast and
/// non-destructive.
pub trait HealthCheck: Send + Sync + 'static {
    /// Run the health check.
    ///
    /// Returns `Ok(())` if the service is healthy, or an error describing
    /// the failure.
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
/// Built with a fluent API and registered in the service registry. The
/// readiness endpoint runs all checks concurrently and reports failures.
///
/// # Example
///
/// ```ignore
/// use modo::health::HealthChecks;
///
/// let checks = HealthChecks::new()
///     .check("read_pool", read_pool.clone())
///     .check("write_pool", write_pool.clone())
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

    /// Register a named health check from a trait impl.
    pub fn check(mut self, name: &str, c: impl HealthCheck) -> Self {
        self.checks.push((name.to_owned(), Arc::new(c)));
        self
    }

    /// Register a named health check from a closure.
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
    pub(crate) fn iter(&self) -> &[(String, Arc<dyn HealthCheck>)] {
        &self.checks
    }
}

impl Default for HealthChecks {
    fn default() -> Self {
        Self::new()
    }
}

use crate::db;

impl HealthCheck for db::Pool {
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async {
            self.acquire()
                .await
                .map_err(|e| crate::Error::internal("pool health check failed").chain(e))?;
            Ok(())
        })
    }
}

impl HealthCheck for db::ReadPool {
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async {
            self.acquire()
                .await
                .map_err(|e| crate::Error::internal("read pool health check failed").chain(e))?;
            Ok(())
        })
    }
}

impl HealthCheck for db::WritePool {
    fn check(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async {
            self.acquire()
                .await
                .map_err(|e| crate::Error::internal("write pool health check failed").chain(e))?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Pool, ReadPool, WritePool};

    #[tokio::test]
    async fn pool_health_check_succeeds() {
        let pool = Pool::new(sqlx::SqlitePool::connect(":memory:").await.unwrap());
        pool.check().await.unwrap();
    }

    #[tokio::test]
    async fn read_pool_health_check_succeeds() {
        let inner = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let pool = ReadPool::new(inner);
        pool.check().await.unwrap();
    }

    #[tokio::test]
    async fn write_pool_health_check_succeeds() {
        let inner = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let pool = WritePool::new(inner);
        pool.check().await.unwrap();
    }

    #[tokio::test]
    async fn check_adds_trait_impl() {
        let pool = Pool::new(sqlx::SqlitePool::connect(":memory:").await.unwrap());
        let checks = HealthChecks::new().check("pool", pool);
        assert_eq!(checks.iter().len(), 1);
        assert_eq!(checks.iter()[0].0, "pool");
        checks.iter()[0].1.check().await.unwrap();
    }

    #[tokio::test]
    async fn fn_health_check_succeeds() {
        let checks = HealthChecks::new().check_fn("ok", || async { Ok(()) });
        let (_, c) = &checks.iter()[0];
        c.check().await.unwrap();
    }

    #[tokio::test]
    async fn fn_health_check_fails() {
        let checks =
            HealthChecks::new().check_fn("fail", || async { Err(crate::Error::internal("down")) });
        let (_, c) = &checks.iter()[0];
        assert!(c.check().await.is_err());
    }

    #[tokio::test]
    async fn chaining_preserves_order() {
        let checks = HealthChecks::new()
            .check_fn("a", || async { Ok(()) })
            .check_fn("b", || async { Ok(()) })
            .check_fn("c", || async { Ok(()) });
        let names: Vec<&str> = checks.iter().iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn health_checks_default_is_empty() {
        let checks = HealthChecks::default();
        assert!(checks.iter().is_empty());
    }
}
