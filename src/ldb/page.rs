use axum::extract::FromRequestParts;
use http::request::Parts;
use serde::{Deserialize, Serialize};

use crate::error::Error;

/// Pagination defaults applied by [`PageRequest`] and [`CursorRequest`]
/// extractors.
///
/// Add an instance to request extensions (via middleware or a layer) to
/// override the defaults. If absent, the extractors fall back to
/// `default_per_page = 20` and `max_per_page = 100`.
#[derive(Debug, Clone)]
pub struct PaginationConfig {
    /// Default number of items per page when `per_page` is not specified.
    pub default_per_page: i64,
    /// Maximum allowed value for `per_page`. Values above this are clamped.
    pub max_per_page: i64,
}

impl Default for PaginationConfig {
    fn default() -> Self {
        Self {
            default_per_page: 20,
            max_per_page: 100,
        }
    }
}

/// Offset-based page response.
///
/// Contains the items for the current page plus metadata for navigating
/// through the full result set. Pages are **1-based**.
///
/// Constructed manually via [`Page::new`].
#[derive(Debug, Serialize)]
pub struct Page<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

impl<T: Serialize> Page<T> {
    /// Build a `Page` from items, total count, current page, and page size.
    pub fn new(items: Vec<T>, total: i64, page: i64, per_page: i64) -> Self {
        let total_pages = if total == 0 || per_page == 0 {
            0
        } else {
            (total + per_page - 1) / per_page
        };
        Self {
            items,
            total,
            page,
            per_page,
            total_pages,
            has_next: page < total_pages,
            has_prev: page > 1,
        }
    }
}

/// Cursor-based page response.
///
/// Uses keyset (cursor) pagination rather than offset-based. The
/// `next_cursor` value should be passed back as the `after` parameter
/// for the next page.
#[derive(Debug, Serialize)]
pub struct CursorPage<T: Serialize> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub per_page: i64,
}

impl<T: Serialize> CursorPage<T> {
    /// Build a `CursorPage` from items, an optional next-cursor, and page
    /// size.
    pub fn new(items: Vec<T>, next_cursor: Option<String>, per_page: i64) -> Self {
        Self {
            has_more: next_cursor.is_some(),
            items,
            next_cursor,
            per_page,
        }
    }
}

/// Offset pagination request extracted from the query string.
///
/// Parsed from `?page=N&per_page=N`. Implements [`FromRequestParts`] so it
/// can be used directly as a handler argument. Values are silently clamped
/// using [`PaginationConfig`] from request extensions (or hardcoded defaults
/// if no config is present).
///
/// Pages are **1-based**. A `page` below `1` is treated as `1`.
#[derive(Debug, Clone, Deserialize)]
pub struct PageRequest {
    #[serde(default = "one")]
    pub page: i64,
    #[serde(default)]
    pub per_page: i64,
}

impl PageRequest {
    /// Clamp values using config.
    pub fn clamp(&mut self, config: &PaginationConfig) {
        if self.page < 1 {
            self.page = 1;
        }
        if self.per_page < 1 {
            self.per_page = config.default_per_page;
        }
        if self.per_page > config.max_per_page {
            self.per_page = config.max_per_page;
        }
    }

    /// Returns the SQL `OFFSET` value for this page.
    pub fn offset(&self) -> i64 {
        (self.page - 1) * self.per_page
    }
}

/// Cursor pagination request extracted from the query string.
///
/// Parsed from `?after=<cursor>&per_page=N`. Implements
/// [`FromRequestParts`] so it can be used directly as a handler argument.
#[derive(Debug, Clone, Deserialize)]
pub struct CursorRequest {
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub per_page: i64,
}

impl CursorRequest {
    /// Clamp values using config.
    pub fn clamp(&mut self, config: &PaginationConfig) {
        if self.per_page < 1 {
            self.per_page = config.default_per_page;
        }
        if self.per_page > config.max_per_page {
            self.per_page = config.max_per_page;
        }
    }
}

fn one() -> i64 {
    1
}

fn resolve_config(parts: &Parts) -> PaginationConfig {
    parts
        .extensions
        .get::<PaginationConfig>()
        .cloned()
        .unwrap_or_default()
}

impl<S: Send + Sync> FromRequestParts<S> for PageRequest {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let config = resolve_config(parts);
        let axum::extract::Query(mut req) =
            axum::extract::Query::<PageRequest>::from_request_parts(parts, state)
                .await
                .map_err(|e| Error::bad_request(format!("invalid pagination params: {e}")))?;
        req.clamp(&config);
        Ok(req)
    }
}

impl<S: Send + Sync> FromRequestParts<S> for CursorRequest {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let config = resolve_config(parts);
        let axum::extract::Query(mut req) =
            axum::extract::Query::<CursorRequest>::from_request_parts(parts, state)
                .await
                .map_err(|e| Error::bad_request(format!("invalid pagination params: {e}")))?;
        req.clamp(&config);
        Ok(req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> PaginationConfig {
        PaginationConfig {
            default_per_page: 20,
            max_per_page: 100,
        }
    }

    #[test]
    fn page_request_defaults() {
        let mut req: PageRequest = serde_urlencoded::from_str("").unwrap();
        req.clamp(&config());
        assert_eq!(req.page, 1);
        assert_eq!(req.per_page, 20);
    }

    #[test]
    fn page_request_zero_page_becomes_one() {
        let mut req = PageRequest {
            page: 0,
            per_page: 10,
        };
        req.clamp(&config());
        assert_eq!(req.page, 1);
    }

    #[test]
    fn page_request_per_page_zero_uses_default() {
        let mut req = PageRequest {
            page: 1,
            per_page: 0,
        };
        req.clamp(&config());
        assert_eq!(req.per_page, 20);
    }

    #[test]
    fn page_request_per_page_over_max_clamped() {
        let mut req = PageRequest {
            page: 1,
            per_page: 999,
        };
        req.clamp(&config());
        assert_eq!(req.per_page, 100);
    }

    #[test]
    fn page_request_valid_values_unchanged() {
        let mut req = PageRequest {
            page: 3,
            per_page: 50,
        };
        req.clamp(&config());
        assert_eq!(req.page, 3);
        assert_eq!(req.per_page, 50);
    }

    #[test]
    fn page_request_offset_calculation() {
        let req = PageRequest {
            page: 3,
            per_page: 10,
        };
        assert_eq!(req.offset(), 20);
    }

    #[test]
    fn page_request_offset_first_page() {
        let req = PageRequest {
            page: 1,
            per_page: 10,
        };
        assert_eq!(req.offset(), 0);
    }

    #[test]
    fn cursor_request_defaults() {
        let mut req: CursorRequest = serde_urlencoded::from_str("").unwrap();
        req.clamp(&config());
        assert!(req.after.is_none());
        assert_eq!(req.per_page, 20);
    }

    #[test]
    fn cursor_request_per_page_over_max_clamped() {
        let mut req = CursorRequest {
            after: None,
            per_page: 500,
        };
        req.clamp(&config());
        assert_eq!(req.per_page, 100);
    }

    #[test]
    fn cursor_request_per_page_zero_becomes_default() {
        let mut req = CursorRequest {
            after: Some("abc".into()),
            per_page: 0,
        };
        req.clamp(&config());
        assert_eq!(req.per_page, 20);
        assert_eq!(req.after.as_deref(), Some("abc"));
    }

    #[test]
    fn page_new_calculates_fields() {
        let page: Page<String> = Page::new(vec!["a".into(), "b".into()], 5, 2, 2);
        assert_eq!(page.total_pages, 3);
        assert!(page.has_next);
        assert!(page.has_prev);
    }

    #[test]
    fn page_new_first_page() {
        let page: Page<String> = Page::new(vec!["a".into(), "b".into()], 10, 1, 2);
        assert_eq!(page.total_pages, 5);
        assert!(page.has_next);
        assert!(!page.has_prev);
    }

    #[test]
    fn page_new_last_page() {
        let page: Page<String> = Page::new(vec!["e".into()], 5, 3, 2);
        assert_eq!(page.total_pages, 3);
        assert!(!page.has_next);
        assert!(page.has_prev);
    }

    #[test]
    fn page_new_empty() {
        let page: Page<String> = Page::new(vec![], 0, 1, 20);
        assert_eq!(page.total_pages, 0);
        assert!(!page.has_next);
        assert!(!page.has_prev);
    }

    #[test]
    fn cursor_page_with_more() {
        let page: CursorPage<String> =
            CursorPage::new(vec!["a".into(), "b".into()], Some("id_b".into()), 2);
        assert!(page.has_more);
        assert_eq!(page.next_cursor.as_deref(), Some("id_b"));
        assert_eq!(page.per_page, 2);
    }

    #[test]
    fn cursor_page_last() {
        let page: CursorPage<String> = CursorPage::new(vec!["a".into()], None, 20);
        assert!(!page.has_more);
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn page_serializes_to_json() {
        let page: Page<i32> = Page::new(vec![1, 2, 3], 10, 1, 3);
        let json = serde_json::to_value(&page).unwrap();
        assert_eq!(json["items"], serde_json::json!([1, 2, 3]));
        assert_eq!(json["total"], 10);
        assert_eq!(json["page"], 1);
        assert_eq!(json["per_page"], 3);
        assert_eq!(json["total_pages"], 4);
        assert_eq!(json["has_next"], true);
        assert_eq!(json["has_prev"], false);
    }

    #[test]
    fn page_request_deserializes_from_query_string() {
        let req: PageRequest = serde_urlencoded::from_str("page=2&per_page=30").unwrap();
        assert_eq!(req.page, 2);
        assert_eq!(req.per_page, 30);
    }

    #[test]
    fn cursor_request_deserializes_from_query_string() {
        let req: CursorRequest = serde_urlencoded::from_str("after=01ABC&per_page=10").unwrap();
        assert_eq!(req.after.as_deref(), Some("01ABC"));
        assert_eq!(req.per_page, 10);
    }

    #[test]
    fn cursor_request_deserializes_without_after() {
        let req: CursorRequest = serde_urlencoded::from_str("per_page=10").unwrap();
        assert!(req.after.is_none());
    }
}
