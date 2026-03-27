use serde::Serialize;
use sqlx::FromRow;
use sqlx::sqlite::SqliteRow;

use crate::db::Reader;
use crate::error::Error;

use super::request::PageRequest;
use super::response::Page;
use super::value::{IntoSqliteValue, SqliteValue, build_args};

/// Builder for offset-based pagination queries.
///
/// Collects a base SQL string and bind parameters, then executes a COUNT
/// query and a data query against a [`Reader`] pool.
pub struct Paginate {
    sql: String,
    args: Vec<SqliteValue>,
    where_sql: Option<String>,
    where_args: Vec<SqliteValue>,
}

impl Paginate {
    /// Create a new builder with the given base SQL query.
    /// The SQL should be a complete SELECT without LIMIT/OFFSET.
    pub fn new(sql: &str) -> Self {
        Self {
            sql: sql.to_owned(),
            args: Vec::new(),
            where_sql: None,
            where_args: Vec::new(),
        }
    }

    /// Bind a parameter matching a `?` placeholder in the SQL.
    pub fn bind(mut self, value: impl IntoSqliteValue) -> Self {
        self.args.push(value.into_sqlite_value());
        self
    }

    /// Append a WHERE clause fragment (for future filter module integration).
    pub fn where_clause(mut self, sql: &str, args: Vec<SqliteValue>) -> Self {
        self.where_sql = Some(sql.to_owned());
        self.where_args = args;
        self
    }

    /// Execute the COUNT and data queries, returning a [`Page<T>`].
    ///
    /// Runs two queries:
    /// 1. `SELECT COUNT(*) FROM ({base_sql}{where_clause})` — total count
    /// 2. `{base_sql}{where_clause} LIMIT ? OFFSET ?` — page items
    pub async fn fetch<T>(
        &self,
        pool: &(impl Reader + Sync),
        req: &PageRequest,
    ) -> crate::Result<Page<T>>
    where
        T: for<'r> FromRow<'r, SqliteRow> + Serialize + Send + Unpin,
    {
        // Count query
        let count_sql = self.count_sql();
        let count_args = self.all_args();
        let (total,): (i64,) = sqlx::query_as_with(&count_sql, count_args)
            .fetch_one(pool.read_pool())
            .await
            .map_err(|e| Error::internal("pagination count query failed").chain(e))?;
        let total = total.max(0) as u64;

        // Data query
        let data_sql = self.data_sql();
        let mut data_args = self.all_args();
        use sqlx::Arguments;
        data_args.add(req.per_page as i64).unwrap();
        data_args.add(req.offset() as i64).unwrap();

        let items: Vec<T> = sqlx::query_as_with(&data_sql, data_args)
            .fetch_all(pool.read_pool())
            .await
            .map_err(|e| Error::internal("pagination data query failed").chain(e))?;

        Ok(Page::new(items, total, req.page, req.per_page))
    }

    fn all_args(&self) -> sqlx::sqlite::SqliteArguments<'static> {
        let combined: Vec<SqliteValue> = self
            .args
            .iter()
            .chain(self.where_args.iter())
            .cloned()
            .collect();
        build_args(&combined)
    }

    pub(crate) fn count_sql(&self) -> String {
        let where_part = self.where_sql.as_deref().unwrap_or("");
        format!("SELECT COUNT(*) FROM ({}{})", self.sql, where_part)
    }

    pub(crate) fn data_sql(&self) -> String {
        let where_part = self.where_sql.as_deref().unwrap_or("");
        format!("{}{} LIMIT ? OFFSET ?", self.sql, where_part)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::value::IntoSqliteValue;

    #[test]
    fn count_sql_wraps_base_query() {
        let p = Paginate::new("SELECT * FROM users WHERE active = ?");
        assert_eq!(
            p.count_sql(),
            "SELECT COUNT(*) FROM (SELECT * FROM users WHERE active = ?)"
        );
    }

    #[test]
    fn data_sql_appends_limit_offset() {
        let p = Paginate::new("SELECT * FROM users");
        assert_eq!(
            p.data_sql(),
            "SELECT * FROM users LIMIT ? OFFSET ?"
        );
    }

    #[test]
    fn where_clause_inserted_between_base_and_limit() {
        let p = Paginate::new("SELECT * FROM users")
            .where_clause(" WHERE active = ?", vec!["true".into_sqlite_value()]);
        assert_eq!(
            p.data_sql(),
            "SELECT * FROM users WHERE active = ? LIMIT ? OFFSET ?"
        );
        assert_eq!(
            p.count_sql(),
            "SELECT COUNT(*) FROM (SELECT * FROM users WHERE active = ?)"
        );
    }
}
