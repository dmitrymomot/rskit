use crate::error::{Error, Result};

use super::conn::ConnExt;
use super::filter::ValidatedFilter;
use super::from_row::FromRow;
use super::page::{CursorPage, CursorRequest, Page, PageRequest};

/// Composable query builder combining filters, sorting, and pagination.
///
/// Created via [`ConnExt::select`] with a base SQL query. Chain
/// [`filter`](Self::filter), [`order_by`](Self::order_by), and
/// [`cursor_column`](Self::cursor_column) before executing with
/// [`page`](Self::page), [`cursor`](Self::cursor), [`fetch_all`](Self::fetch_all),
/// [`fetch_one`](Self::fetch_one), or [`fetch_optional`](Self::fetch_optional).
pub struct SelectBuilder<'a, C: ConnExt> {
    conn: &'a C,
    base_sql: String,
    filter: Option<ValidatedFilter>,
    order_by: Option<String>,
    cursor_column: String,
}

impl<'a, C: ConnExt> SelectBuilder<'a, C> {
    pub(crate) fn new(conn: &'a C, sql: &str) -> Self {
        Self {
            conn,
            base_sql: sql.to_string(),
            filter: None,
            order_by: None,
            cursor_column: "id".to_string(),
        }
    }

    /// Apply a validated filter (WHERE clauses).
    pub fn filter(mut self, filter: ValidatedFilter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Set ORDER BY clause. This is raw SQL — not user input.
    /// If a filter has a sort_clause, it takes precedence over this.
    pub fn order_by(mut self, order: &str) -> Self {
        self.order_by = Some(order.to_string());
        self
    }

    /// Set the column used for cursor pagination (default: `"id"`).
    ///
    /// The column must appear in the SELECT list and be sortable (e.g., ULID,
    /// timestamp, auto-increment). Cursor pagination will ORDER BY this column
    /// ascending and use it for the `WHERE col > ?` condition.
    pub fn cursor_column(mut self, col: &str) -> Self {
        self.cursor_column = col.to_string();
        self
    }

    /// Build WHERE clause and params from filter.
    fn build_where(&self) -> (String, Vec<libsql::Value>) {
        match &self.filter {
            Some(f) if !f.clauses.is_empty() => {
                let where_sql = format!(" WHERE {}", f.clauses.join(" AND "));
                (where_sql, f.params.clone())
            }
            _ => (String::new(), Vec::new()),
        }
    }

    /// Resolve ORDER BY — filter sort takes precedence, then explicit order_by.
    fn resolve_order(&self) -> Option<String> {
        self.filter
            .as_ref()
            .and_then(|f| f.sort_clause.clone())
            .or_else(|| self.order_by.clone())
    }

    /// Execute with offset pagination, returning a [`Page<T>`].
    ///
    /// Runs a `COUNT(*)` subquery for the total, then fetches the
    /// requested page with `LIMIT`/`OFFSET`.
    ///
    /// # Errors
    ///
    /// Returns an error if the query or row conversion fails.
    pub async fn page<T: FromRow + serde::Serialize>(self, req: PageRequest) -> Result<Page<T>> {
        let (where_sql, mut params) = self.build_where();
        let order = self.resolve_order();

        // Count query
        let count_sql = format!(
            "SELECT COUNT(*) FROM ({}{}) AS _count",
            self.base_sql, where_sql
        );
        let mut rows = self
            .conn
            .query_raw(&count_sql, params.clone())
            .await
            .map_err(Error::from)?;
        let total: i64 = rows
            .next()
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::internal("count query returned no rows"))?
            .get(0)
            .map_err(Error::from)?;

        // Data query
        let order_sql = order.map(|o| format!(" ORDER BY {o}")).unwrap_or_default();
        let data_sql = format!(
            "{}{}{} LIMIT ? OFFSET ?",
            self.base_sql, where_sql, order_sql
        );
        params.push(libsql::Value::from(req.per_page));
        params.push(libsql::Value::from(req.offset()));

        let mut rows = self
            .conn
            .query_raw(&data_sql, params)
            .await
            .map_err(Error::from)?;
        let mut items = Vec::new();
        while let Some(row) = rows.next().await.map_err(Error::from)? {
            items.push(T::from_row(&row)?);
        }

        Ok(Page::new(items, total, req.page, req.per_page))
    }

    /// Execute with cursor pagination. Returns [`CursorPage<T>`].
    ///
    /// Uses the configured cursor column (default `"id"`) for ordering and
    /// the `WHERE col > ?` condition. Set a different column with
    /// [`cursor_column`](Self::cursor_column).
    pub async fn cursor<T: FromRow + serde::Serialize>(
        self,
        req: CursorRequest,
    ) -> Result<CursorPage<T>> {
        let (where_sql, mut params) = self.build_where();
        let col = &self.cursor_column;

        // Add cursor condition
        let cursor_condition = if let Some(ref after) = req.after {
            params.push(libsql::Value::from(after.clone()));
            if where_sql.is_empty() {
                format!(" WHERE \"{col}\" > ?")
            } else {
                format!(" AND \"{col}\" > ?")
            }
        } else {
            String::new()
        };

        // Fetch one extra to determine has_more
        let limit = req.per_page + 1;
        let sql = format!(
            "{}{}{} ORDER BY \"{col}\" ASC LIMIT ?",
            self.base_sql, where_sql, cursor_condition
        );
        params.push(libsql::Value::from(limit));

        let mut rows = self
            .conn
            .query_raw(&sql, params)
            .await
            .map_err(Error::from)?;

        // Track cursor values alongside items for cursor extraction.
        // Find the cursor column index dynamically on the first row.
        let mut items = Vec::new();
        let mut cursor_values: Vec<Option<String>> = Vec::new();
        let mut cursor_col_idx: Option<i32> = None;
        while let Some(row) = rows.next().await.map_err(Error::from)? {
            if cursor_col_idx.is_none() {
                cursor_col_idx = Some(
                    (0..row.column_count())
                        .find(|&i| row.column_name(i) == Some(col))
                        .ok_or_else(|| {
                            Error::internal(format!(
                                "cursor column '{col}' not found in result set"
                            ))
                        })?,
                );
            }
            let idx = cursor_col_idx.unwrap();
            let cursor_val = match row.get_value(idx) {
                Ok(libsql::Value::Text(s)) => Some(s),
                Ok(libsql::Value::Integer(n)) => Some(n.to_string()),
                Ok(libsql::Value::Real(f)) => Some(f.to_string()),
                _ => None,
            };
            cursor_values.push(cursor_val);
            items.push(T::from_row(&row)?);
        }

        let has_more = items.len() as i64 > req.per_page;
        if has_more {
            items.pop();
            cursor_values.pop();
        }

        let next_cursor = if has_more {
            cursor_values.last().cloned().flatten()
        } else {
            None
        };

        Ok(CursorPage {
            items,
            next_cursor,
            has_more,
            per_page: req.per_page,
        })
    }

    /// Execute without pagination, returning all matching rows.
    ///
    /// # Errors
    ///
    /// Returns an error if the query or row conversion fails.
    pub async fn fetch_all<T: FromRow>(self) -> Result<Vec<T>> {
        let (where_sql, params) = self.build_where();
        let order = self.resolve_order();
        let order_sql = order.map(|o| format!(" ORDER BY {o}")).unwrap_or_default();
        let sql = format!("{}{}{}", self.base_sql, where_sql, order_sql);

        let mut rows = self
            .conn
            .query_raw(&sql, params)
            .await
            .map_err(Error::from)?;
        let mut items = Vec::new();
        while let Some(row) = rows.next().await.map_err(Error::from)? {
            items.push(T::from_row(&row)?);
        }
        Ok(items)
    }

    /// Execute without pagination, returning the first row.
    ///
    /// # Errors
    ///
    /// Returns [`Error::not_found`](crate::Error::not_found) if no rows match.
    pub async fn fetch_one<T: FromRow>(self) -> Result<T> {
        let (where_sql, params) = self.build_where();
        let sql = format!("{}{} LIMIT 1", self.base_sql, where_sql);

        let mut rows = self
            .conn
            .query_raw(&sql, params)
            .await
            .map_err(Error::from)?;
        let row = rows
            .next()
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::not_found("record not found"))?;
        T::from_row(&row)
    }

    /// Execute without pagination, returning the first row or `None`.
    ///
    /// # Errors
    ///
    /// Returns an error if the query or row conversion fails.
    pub async fn fetch_optional<T: FromRow>(self) -> Result<Option<T>> {
        let (where_sql, params) = self.build_where();
        let sql = format!("{}{} LIMIT 1", self.base_sql, where_sql);

        let mut rows = self
            .conn
            .query_raw(&sql, params)
            .await
            .map_err(Error::from)?;
        match rows.next().await.map_err(Error::from)? {
            Some(row) => Ok(Some(T::from_row(&row)?)),
            None => Ok(None),
        }
    }
}
