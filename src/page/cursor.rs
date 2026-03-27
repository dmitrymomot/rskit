use serde::Serialize;
use sqlx::FromRow;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::db::Reader;
use crate::error::Error;

use super::request::CursorRequest;
use super::response::CursorPage;
use super::value::{IntoSqliteValue, SqliteValue, build_args};

/// Builder for ID-based keyset (cursor) pagination queries.
///
/// Uses the `id` column for cursor positioning. Returns newest items first
/// by default; call [`.oldest_first()`](Self::oldest_first) to reverse.
pub struct CursorPaginate {
    sql: String,
    args: Vec<SqliteValue>,
    oldest_first: bool,
    where_sql: Option<String>,
    where_args: Vec<SqliteValue>,
}

impl CursorPaginate {
    /// Create a new builder. SQL must include an `id` column.
    /// Do NOT add ORDER BY or LIMIT — those are appended automatically.
    pub fn new(sql: &str) -> Self {
        Self {
            sql: sql.to_owned(),
            args: Vec::new(),
            oldest_first: false,
            where_sql: None,
            where_args: Vec::new(),
        }
    }

    /// Bind a parameter to the base SQL query.
    pub fn bind(mut self, value: impl IntoSqliteValue) -> Self {
        self.args.push(value.into_sqlite_value());
        self
    }

    /// Sort oldest-first (`ORDER BY id ASC`). Default is newest-first.
    pub fn oldest_first(mut self) -> Self {
        self.oldest_first = true;
        self
    }

    /// Append a WHERE clause fragment (for future filter module).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn where_clause(mut self, sql: &str, args: Vec<SqliteValue>) -> Self {
        self.where_sql = Some(sql.to_owned());
        self.where_args = args;
        self
    }

    /// Execute the query and return a [`CursorPage<T>`].
    ///
    /// Fetches `per_page + 1` rows to detect has_more. Reads the `id`
    /// column from the last row to produce the `next` cursor.
    pub async fn fetch<T>(
        &self,
        pool: &(impl Reader + Sync),
        req: &CursorRequest,
    ) -> crate::Result<CursorPage<T>>
    where
        T: for<'r> FromRow<'r, SqliteRow> + Serialize + Send + Unpin,
    {
        let (query_sql, _) = self.build_query(req);
        let args = self.build_args(req);

        let rows: Vec<SqliteRow> = sqlx::query_with(&query_sql, args)
            .fetch_all(pool.read_pool())
            .await
            .map_err(|e| Error::internal("cursor pagination query failed").chain(e))?;

        let has_more = rows.len() > req.per_page as usize;
        let take = if has_more {
            req.per_page as usize
        } else {
            rows.len()
        };

        let next = if has_more {
            rows.get(take - 1).map(|r| r.get::<String, _>("id"))
        } else {
            None
        };

        let items: Vec<T> = rows
            .into_iter()
            .take(take)
            .map(|r| T::from_row(&r))
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| Error::internal("row deserialization failed").chain(e))?;

        Ok(CursorPage::new(items, next, req.per_page))
    }

    /// Build the SQL string and return it with the LIMIT value (for testing).
    pub(crate) fn build_query(&self, req: &CursorRequest) -> (String, i64) {
        let (op, order) = if self.oldest_first {
            (">", "ASC")
        } else {
            ("<", "DESC")
        };
        let limit = req.per_page as i64 + 1;
        let where_part = self.where_sql.as_deref().unwrap_or("");
        let inner = format!("{}{}", self.sql, where_part);

        let sql = if req.after.is_some() {
            format!("SELECT * FROM ({inner}) WHERE id {op} ? ORDER BY id {order} LIMIT ?")
        } else {
            format!("SELECT * FROM ({inner}) ORDER BY id {order} LIMIT ?")
        };

        (sql, limit)
    }

    fn build_args(&self, req: &CursorRequest) -> sqlx::sqlite::SqliteArguments<'static> {
        let mut values: Vec<SqliteValue> = self
            .args
            .iter()
            .chain(self.where_args.iter())
            .cloned()
            .collect();

        if let Some(ref cursor_id) = req.after {
            values.push(SqliteValue::Text(cursor_id.clone()));
        }

        let limit = req.per_page as i64 + 1;
        values.push(SqliteValue::Int64(limit));

        build_args(&values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::value::IntoSqliteValue;

    #[test]
    fn first_page_sql_newest_first() {
        let p = CursorPaginate::new("SELECT * FROM events");
        let req = CursorRequest {
            after: None,
            per_page: 20,
        };
        let (sql, limit) = p.build_query(&req);
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT * FROM events) ORDER BY id DESC LIMIT ?"
        );
        assert_eq!(limit, 21);
    }

    #[test]
    fn next_page_sql_newest_first() {
        let p = CursorPaginate::new("SELECT * FROM events");
        let req = CursorRequest {
            after: Some("abc".into()),
            per_page: 20,
        };
        let (sql, _) = p.build_query(&req);
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT * FROM events) WHERE id < ? ORDER BY id DESC LIMIT ?"
        );
    }

    #[test]
    fn first_page_sql_oldest_first() {
        let p = CursorPaginate::new("SELECT * FROM events").oldest_first();
        let req = CursorRequest {
            after: None,
            per_page: 10,
        };
        let (sql, _) = p.build_query(&req);
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT * FROM events) ORDER BY id ASC LIMIT ?"
        );
    }

    #[test]
    fn next_page_sql_oldest_first() {
        let p = CursorPaginate::new("SELECT * FROM events").oldest_first();
        let req = CursorRequest {
            after: Some("abc".into()),
            per_page: 10,
        };
        let (sql, _) = p.build_query(&req);
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT * FROM events) WHERE id > ? ORDER BY id ASC LIMIT ?"
        );
    }

    #[test]
    fn where_clause_included() {
        let p = CursorPaginate::new("SELECT * FROM events")
            .where_clause(" WHERE tenant_id = ?", vec!["t1".into_sqlite_value()]);
        let req = CursorRequest {
            after: None,
            per_page: 5,
        };
        let (sql, _) = p.build_query(&req);
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT * FROM events WHERE tenant_id = ?) ORDER BY id DESC LIMIT ?"
        );
    }

    #[test]
    fn limit_is_per_page_plus_one() {
        let p = CursorPaginate::new("SELECT * FROM events");
        let req = CursorRequest {
            after: None,
            per_page: 5,
        };
        let (_, limit) = p.build_query(&req);
        assert_eq!(limit, 6);
    }
}
