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
/// Uses the `id` column for cursor positioning. The column must be a unique,
/// lexicographically ordered primary key (e.g., ULID). Duplicate or
/// non-monotone IDs will cause skipped or repeated rows.
///
/// Returns newest items first by default; call
/// [`.oldest_first()`](Self::oldest_first) to reverse.
pub struct CursorPaginate {
    sql: String,
    args: Vec<SqliteValue>,
    oldest_first: bool,
}

impl CursorPaginate {
    /// Create a new builder with the given base SQL query.
    ///
    /// The query must return an `id TEXT` column that is a unique,
    /// lexicographically ordered primary key. Do **not** add `ORDER BY` or
    /// `LIMIT` — those are appended automatically.
    pub fn new(sql: &str) -> Self {
        Self {
            sql: sql.to_owned(),
            args: Vec::new(),
            oldest_first: false,
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
        let limit = req.per_page as i64 + 1;
        let query_sql = self.build_sql(req);
        let args = self.build_args(req, limit);

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

    /// Build the SQL string for the cursor query.
    pub(crate) fn build_sql(&self, req: &CursorRequest) -> String {
        let (op, order) = if self.oldest_first {
            (">", "ASC")
        } else {
            ("<", "DESC")
        };
        let inner = &self.sql;

        if req.after.is_some() {
            format!("SELECT * FROM ({inner}) WHERE id {op} ? ORDER BY id {order} LIMIT ?")
        } else {
            format!("SELECT * FROM ({inner}) ORDER BY id {order} LIMIT ?")
        }
    }

    fn build_args(
        &self,
        req: &CursorRequest,
        limit: i64,
    ) -> sqlx::sqlite::SqliteArguments<'static> {
        let mut values: Vec<SqliteValue> = self.args.clone();

        if let Some(ref cursor_id) = req.after {
            values.push(SqliteValue::Text(cursor_id.clone()));
        }

        values.push(SqliteValue::Int64(limit));

        build_args(&values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_page_sql_newest_first() {
        let p = CursorPaginate::new("SELECT * FROM events");
        let req = CursorRequest {
            after: None,
            per_page: 20,
        };
        let sql = p.build_sql(&req);
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT * FROM events) ORDER BY id DESC LIMIT ?"
        );
    }

    #[test]
    fn next_page_sql_newest_first() {
        let p = CursorPaginate::new("SELECT * FROM events");
        let req = CursorRequest {
            after: Some("abc".into()),
            per_page: 20,
        };
        let sql = p.build_sql(&req);
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
        let sql = p.build_sql(&req);
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
        let sql = p.build_sql(&req);
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT * FROM events) WHERE id > ? ORDER BY id ASC LIMIT ?"
        );
    }
}
