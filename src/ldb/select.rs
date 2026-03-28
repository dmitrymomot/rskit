use crate::error::{Error, Result};

use super::conn::ConnExt;
use super::filter::ValidatedFilter;
use super::from_row::FromRow;
use super::page::{CursorPage, CursorRequest, Page, PageRequest};

/// Composable query builder for filter + pagination.
pub struct SelectBuilder<'a, C: ConnExt> {
    conn: &'a C,
    base_sql: String,
    filter: Option<ValidatedFilter>,
    order_by: Option<String>,
}

impl<'a, C: ConnExt> SelectBuilder<'a, C> {
    pub(crate) fn new(conn: &'a C, sql: &str) -> Self {
        Self {
            conn,
            base_sql: sql.to_string(),
            filter: None,
            order_by: None,
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

    /// Execute with offset pagination. Returns Page<T>.
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

    /// Execute with cursor pagination. Returns CursorPage<T>.
    ///
    /// Assumes the first column in the SELECT is the cursor ID (e.g., ULID).
    pub async fn cursor<T: FromRow + serde::Serialize>(
        self,
        req: CursorRequest,
    ) -> Result<CursorPage<T>> {
        let (where_sql, mut params) = self.build_where();

        // Add cursor condition
        let cursor_condition = if let Some(ref after) = req.after {
            params.push(libsql::Value::from(after.clone()));
            if where_sql.is_empty() {
                " WHERE id > ?".to_string()
            } else {
                " AND id > ?".to_string()
            }
        } else {
            String::new()
        };

        // Fetch one extra to determine has_more
        let limit = req.per_page + 1;
        let sql = format!(
            "{}{}{} ORDER BY id ASC LIMIT ?",
            self.base_sql, where_sql, cursor_condition
        );
        params.push(libsql::Value::from(limit));

        let mut rows = self
            .conn
            .query_raw(&sql, params)
            .await
            .map_err(Error::from)?;

        // Track IDs alongside items for cursor extraction
        let mut items = Vec::new();
        let mut ids: Vec<Option<String>> = Vec::new();
        while let Some(row) = rows.next().await.map_err(Error::from)? {
            ids.push(row.get::<String>(0).ok());
            items.push(T::from_row(&row)?);
        }

        let has_more = items.len() as i64 > req.per_page;
        if has_more {
            items.pop();
            ids.pop();
        }

        let next_cursor = if has_more {
            ids.last().cloned().flatten()
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

    /// Execute without pagination. Returns Vec<T>.
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

    /// Execute without pagination. Returns first row.
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

    /// Execute without pagination. Returns Option<T>.
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
