use axum::extract::FromRequestParts;
use http::request::Parts;
use serde::Deserialize;

use super::config::PaginationConfig;
use crate::error::Error;

/// Offset pagination parameters extracted from the query string.
///
/// Parsed from `?page=2&per_page=20`. Implements [`FromRequestParts`] so it
/// can be used directly as a handler argument. Values are silently clamped
/// using [`PaginationConfig`] from request extensions (or hardcoded defaults
/// if no config is present).
///
/// Pages are **1-based**. A `page` of `0` is treated as `1`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PageRequest {
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub per_page: u32,
}

/// Cursor pagination parameters extracted from the query string.
///
/// Parsed from `?after=01JQDKV...&per_page=20`. Implements
/// [`FromRequestParts`] so it can be used directly as a handler argument.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CursorRequest {
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub per_page: u32,
}

impl PageRequest {
    pub(crate) fn clamp(mut self, config: &PaginationConfig) -> Self {
        if self.page == 0 {
            self.page = 1;
        }
        if self.per_page == 0 {
            self.per_page = config.default_per_page;
        }
        self.per_page = self.per_page.clamp(1, config.max_per_page);
        self
    }

    /// Returns the SQL `OFFSET` value for this page.
    pub fn offset(&self) -> u64 {
        (self.page.saturating_sub(1) as u64) * (self.per_page as u64)
    }
}

impl CursorRequest {
    pub(crate) fn clamp(mut self, config: &PaginationConfig) -> Self {
        if self.per_page == 0 {
            self.per_page = config.default_per_page;
        }
        self.per_page = self.per_page.clamp(1, config.max_per_page);
        self
    }
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
        let axum::extract::Query(raw) =
            axum::extract::Query::<PageRequest>::from_request_parts(parts, state)
                .await
                .map_err(|e| Error::bad_request(format!("invalid pagination params: {e}")))?;
        Ok(raw.clamp(&config))
    }
}

impl<S: Send + Sync> FromRequestParts<S> for CursorRequest {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let config = resolve_config(parts);
        let axum::extract::Query(raw) =
            axum::extract::Query::<CursorRequest>::from_request_parts(parts, state)
                .await
                .map_err(|e| Error::bad_request(format!("invalid pagination params: {e}")))?;
        Ok(raw.clamp(&config))
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
        let req = PageRequest::default();
        let req = req.clamp(&config());
        assert_eq!(req.page, 1);
        assert_eq!(req.per_page, 20);
    }

    #[test]
    fn page_request_zero_page_becomes_one() {
        let req = PageRequest {
            page: 0,
            per_page: 10,
        };
        let req = req.clamp(&config());
        assert_eq!(req.page, 1);
    }

    #[test]
    fn page_request_per_page_zero_uses_default() {
        let req = PageRequest {
            page: 1,
            per_page: 0,
        };
        let req = req.clamp(&config());
        assert_eq!(req.per_page, 20); // config default, not 1
    }

    #[test]
    fn page_request_per_page_over_max_clamped() {
        let req = PageRequest {
            page: 1,
            per_page: 999,
        };
        let req = req.clamp(&config());
        assert_eq!(req.per_page, 100);
    }

    #[test]
    fn page_request_valid_values_unchanged() {
        let req = PageRequest {
            page: 3,
            per_page: 50,
        };
        let req = req.clamp(&config());
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
        let req = CursorRequest::default();
        let req = req.clamp(&config());
        assert!(req.after.is_none());
        assert_eq!(req.per_page, 20);
    }

    #[test]
    fn cursor_request_per_page_over_max_clamped() {
        let req = CursorRequest {
            after: None,
            per_page: 500,
        };
        let req = req.clamp(&config());
        assert_eq!(req.per_page, 100);
    }

    #[test]
    fn cursor_request_per_page_zero_becomes_default() {
        let req = CursorRequest {
            after: Some("abc".into()),
            per_page: 0,
        };
        let req = req.clamp(&config());
        assert_eq!(req.per_page, 20);
        assert_eq!(req.after.as_deref(), Some("abc"));
    }

    #[test]
    fn page_request_deserializes_from_query_string() {
        let req: PageRequest = serde_urlencoded::from_str("page=2&per_page=30").unwrap();
        assert_eq!(req.page, 2);
        assert_eq!(req.per_page, 30);
    }

    #[test]
    fn page_request_deserializes_empty_query_string() {
        let req: PageRequest = serde_urlencoded::from_str("").unwrap();
        assert_eq!(req.page, 0); // pre-clamp default
        assert_eq!(req.per_page, 0); // pre-clamp default
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
