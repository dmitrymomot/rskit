use libsql::params::IntoParams;

use crate::error::{Error, Result};

use super::from_row::FromRow;

/// Low-level query trait implemented for `libsql::Connection` and `libsql::Transaction`.
///
/// Provides `query_raw`, `execute_raw`, and the [`select`](Self::select) builder
/// entry point. Higher-level helpers are on [`ConnQueryExt`], which is blanket-implemented
/// for all `ConnExt` types.
pub trait ConnExt: Sync {
    /// Execute a query and return raw rows.
    fn query_raw(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl std::future::Future<Output = std::result::Result<libsql::Rows, libsql::Error>> + Send;

    /// Execute a statement and return the number of affected rows.
    fn execute_raw(
        &self,
        sql: &str,
        params: impl IntoParams + Send,
    ) -> impl std::future::Future<Output = std::result::Result<u64, libsql::Error>> + Send;

    /// Start a composable [`SelectBuilder`](super::SelectBuilder) from a base SQL query.
    fn select<'a>(&'a self, sql: &str) -> super::select::SelectBuilder<'a, Self>
    where
        Self: Sized,
    {
        super::select::SelectBuilder::new(self, sql)
    }
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
    ) -> impl std::future::Future<Output = std::result::Result<u64, libsql::Error>> + Send {
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
    ) -> impl std::future::Future<Output = std::result::Result<u64, libsql::Error>> + Send {
        self.execute(sql, params)
    }
}

/// High-level query helpers built on [`ConnExt`].
///
/// Blanket-implemented for all `ConnExt` types. Import this trait to use
/// `query_one`, `query_optional`, `query_all`, and their `_map` variants.
pub trait ConnQueryExt: ConnExt {
    /// Fetch the first row as `T` via [`FromRow`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::not_found`](crate::Error::not_found) if the query returns no rows.
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

    /// Fetch the first row as `T` via [`FromRow`], returning `None` if empty.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails or row conversion fails.
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

    /// Fetch all rows as `Vec<T>` via [`FromRow`].
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails or any row conversion fails.
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

    /// Fetch the first row and map it with a closure.
    ///
    /// # Errors
    ///
    /// Returns [`Error::not_found`](crate::Error::not_found) if the query returns no rows.
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

    /// Fetch the first row and map it with a closure, returning `None` if empty.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails or the mapping closure returns an error.
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

    /// Fetch all rows and map each with a closure.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails or any mapping closure call returns an error.
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
