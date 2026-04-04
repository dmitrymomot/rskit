use crate::error::{Error, Result};

/// Database health metrics from PRAGMA introspection.
///
/// Contains page-level statistics useful for deciding whether to run
/// `VACUUM`. Does **not** derive `Serialize` — these are internal
/// infrastructure metrics that must not be exposed on unauthenticated
/// endpoints.
#[derive(Debug, Clone)]
pub struct DbHealth {
    /// Total number of pages in the database.
    pub page_count: u64,
    /// Number of pages on the freelist (reclaimable by VACUUM).
    pub freelist_count: u64,
    /// Size of each page in bytes.
    pub page_size: u64,
    /// Percentage of pages on the freelist (0.0–100.0).
    pub free_percent: f64,
    /// Total database file size in bytes (`page_count * page_size`).
    pub total_size_bytes: u64,
    /// Wasted space in bytes (`freelist_count * page_size`).
    pub wasted_bytes: u64,
}

impl DbHealth {
    /// Collect health metrics via `PRAGMA page_count`, `freelist_count`,
    /// `page_size`. Computes derived fields from those three values.
    pub async fn collect(conn: &libsql::Connection) -> Result<Self> {
        let page_count = Self::pragma_u64(conn, "page_count").await?;
        let freelist_count = Self::pragma_u64(conn, "freelist_count").await?;
        let page_size = Self::pragma_u64(conn, "page_size").await?;

        let free_percent = if page_count > 0 {
            (freelist_count as f64 / page_count as f64) * 100.0
        } else {
            0.0
        };

        Ok(Self {
            page_count,
            freelist_count,
            page_size,
            free_percent,
            total_size_bytes: page_count * page_size,
            wasted_bytes: freelist_count * page_size,
        })
    }

    /// Returns `true` if `free_percent >= threshold_percent`.
    pub fn needs_vacuum(&self, threshold_percent: f64) -> bool {
        self.free_percent >= threshold_percent
    }

    async fn pragma_u64(conn: &libsql::Connection, name: &str) -> Result<u64> {
        let mut rows = conn
            .query(&format!("PRAGMA {name}"), ())
            .await
            .map_err(Error::from)?;
        let row = rows
            .next()
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::internal(format!("PRAGMA {name} returned no rows")))?;
        let val: i64 = row.get(0).map_err(Error::from)?;
        Ok(val as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_conn() -> libsql::Connection {
        let db = libsql::Builder::new_local(":memory:")
            .build()
            .await
            .unwrap();
        db.connect().unwrap()
    }

    #[tokio::test]
    async fn collect_returns_metrics_for_fresh_db() {
        let conn = test_conn().await;
        // Create a table to force page allocation in the in-memory database.
        conn.execute("CREATE TABLE _health_probe (id INTEGER PRIMARY KEY)", ())
            .await
            .unwrap();
        let health = DbHealth::collect(&conn).await.unwrap();

        assert!(health.page_count > 0);
        assert_eq!(health.freelist_count, 0);
        assert_eq!(health.page_size, 4096);
        assert_eq!(health.free_percent, 0.0);
        assert_eq!(health.total_size_bytes, health.page_count * 4096);
        assert_eq!(health.wasted_bytes, 0);
    }

    #[tokio::test]
    async fn needs_vacuum_threshold_logic() {
        let health = DbHealth {
            page_count: 100,
            freelist_count: 25,
            page_size: 4096,
            free_percent: 25.0,
            total_size_bytes: 100 * 4096,
            wasted_bytes: 25 * 4096,
        };

        assert!(health.needs_vacuum(20.0));
        assert!(health.needs_vacuum(25.0));
        assert!(!health.needs_vacuum(30.0));
    }
}
