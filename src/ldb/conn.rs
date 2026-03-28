use libsql::params::IntoParams;

use crate::error::{Error, Result};

use super::from_row::FromRow;

/// Extension trait adding query helpers to libsql::Connection and libsql::Transaction.
pub trait ConnExt: Sync {
    fn query_raw(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl std::future::Future<Output = std::result::Result<libsql::Rows, libsql::Error>> + Send;

    fn execute_raw(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl std::future::Future<Output = std::result::Result<u64, libsql::Error>> + Send;
}

impl ConnExt for libsql::Connection {
    fn query_raw(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl std::future::Future<Output = std::result::Result<libsql::Rows, libsql::Error>> + Send
    {
        self.query(sql, params)
    }

    fn execute_raw(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl std::future::Future<Output = std::result::Result<u64, libsql::Error>> + Send
    {
        self.execute(sql, params)
    }
}

impl ConnExt for libsql::Transaction {
    fn query_raw(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl std::future::Future<Output = std::result::Result<libsql::Rows, libsql::Error>> + Send
    {
        self.query(sql, params)
    }

    fn execute_raw(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl std::future::Future<Output = std::result::Result<u64, libsql::Error>> + Send
    {
        self.execute(sql, params)
    }
}

/// High-level query helpers. Import this trait to use them.
pub trait ConnQueryExt: ConnExt {
    /// Fetch first row as T via FromRow. Returns Error::not_found if empty.
    fn query_one<T: FromRow + Send>(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl std::future::Future<Output = Result<T>> + Send {
        async move {
            let mut rows = self.query_raw(sql, params).await.map_err(Error::from)?;
            let row = rows
                .next()
                .await
                .map_err(Error::from)?
                .ok_or_else(|| Error::not_found("record not found"))?;
            T::from_row(&row)
        }
    }

    /// Fetch first row as T via FromRow. Returns None if empty.
    fn query_optional<T: FromRow + Send>(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl std::future::Future<Output = Result<Option<T>>> + Send {
        async move {
            let mut rows = self.query_raw(sql, params).await.map_err(Error::from)?;
            match rows.next().await.map_err(Error::from)? {
                Some(row) => Ok(Some(T::from_row(&row)?)),
                None => Ok(None),
            }
        }
    }

    /// Fetch all rows as Vec<T> via FromRow.
    fn query_all<T: FromRow + Send>(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl std::future::Future<Output = Result<Vec<T>>> + Send {
        async move {
            let mut rows = self.query_raw(sql, params).await.map_err(Error::from)?;
            let mut result = Vec::new();
            while let Some(row) = rows.next().await.map_err(Error::from)? {
                result.push(T::from_row(&row)?);
            }
            Ok(result)
        }
    }

    /// Fetch first row, map with closure. Returns Error::not_found if empty.
    fn query_one_map<T: Send, F>(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
        f: F,
    ) -> impl std::future::Future<Output = Result<T>> + Send
    where
        F: Fn(&libsql::Row) -> Result<T> + Send,
    {
        async move {
            let mut rows = self.query_raw(sql, params).await.map_err(Error::from)?;
            let row = rows
                .next()
                .await
                .map_err(Error::from)?
                .ok_or_else(|| Error::not_found("record not found"))?;
            f(&row)
        }
    }

    /// Fetch first row, map with closure. Returns None if empty.
    fn query_optional_map<T: Send, F>(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
        f: F,
    ) -> impl std::future::Future<Output = Result<Option<T>>> + Send
    where
        F: Fn(&libsql::Row) -> Result<T> + Send,
    {
        async move {
            let mut rows = self.query_raw(sql, params).await.map_err(Error::from)?;
            match rows.next().await.map_err(Error::from)? {
                Some(row) => Ok(Some(f(&row)?)),
                None => Ok(None),
            }
        }
    }

    /// Fetch all rows, map with closure.
    fn query_all_map<T: Send, F>(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
        f: F,
    ) -> impl std::future::Future<Output = Result<Vec<T>>> + Send
    where
        F: Fn(&libsql::Row) -> Result<T> + Send,
    {
        async move {
            let mut rows = self.query_raw(sql, params).await.map_err(Error::from)?;
            let mut result = Vec::new();
            while let Some(row) = rows.next().await.map_err(Error::from)? {
                result.push(f(&row)?);
            }
            Ok(result)
        }
    }
}

// Blanket implementation: anything that implements ConnExt gets ConnQueryExt for free
impl<T: ConnExt> ConnQueryExt for T {}
