use serde::Serialize;

/// Offset-paginated result set.
///
/// Contains the items for the current page plus metadata for navigating
/// through the full result set. Pages are **1-based**.
///
/// Constructed by [`super::Paginate::fetch`] or manually via [`Page::new`].
#[derive(Debug, Clone, Serialize)]
pub struct Page<T: Serialize> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
    pub has_next: bool,
    pub has_prev: bool,
}

impl<T: Serialize> Page<T> {
    /// Build a `Page` from items, total count, current page, and page size.
    pub fn new(items: Vec<T>, total: u64, page: u32, per_page: u32) -> Self {
        let total_pages = if total == 0 {
            0
        } else {
            ((total + per_page as u64 - 1) / per_page as u64) as u32
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

/// Cursor-paginated result set using ID-based keyset pagination.
///
/// Constructed by [`super::CursorPaginate::fetch`] or manually via [`CursorPage::new`].
#[derive(Debug, Clone, Serialize)]
pub struct CursorPage<T: Serialize> {
    pub items: Vec<T>,
    pub next: Option<String>,
    pub has_more: bool,
    pub per_page: u32,
}

impl<T: Serialize> CursorPage<T> {
    /// Build a `CursorPage` from items, an optional next-cursor, and page size.
    pub fn new(items: Vec<T>, next: Option<String>, per_page: u32) -> Self {
        Self {
            has_more: next.is_some(),
            items,
            next,
            per_page,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_metadata_first_page() {
        let page: Page<String> = Page::new(vec!["a".into(), "b".into()], 10, 1, 2);
        assert_eq!(page.total, 10);
        assert_eq!(page.page, 1);
        assert_eq!(page.per_page, 2);
        assert_eq!(page.total_pages, 5);
        assert!(page.has_next);
        assert!(!page.has_prev);
    }

    #[test]
    fn page_metadata_last_page() {
        let page: Page<String> = Page::new(vec!["e".into()], 5, 3, 2);
        assert_eq!(page.total_pages, 3);
        assert!(!page.has_next);
        assert!(page.has_prev);
    }

    #[test]
    fn page_metadata_single_page() {
        let page: Page<String> = Page::new(vec!["a".into()], 1, 1, 20);
        assert_eq!(page.total_pages, 1);
        assert!(!page.has_next);
        assert!(!page.has_prev);
    }

    #[test]
    fn page_metadata_empty() {
        let page: Page<String> = Page::new(vec![], 0, 1, 20);
        assert_eq!(page.total_pages, 0);
        assert!(!page.has_next);
        assert!(!page.has_prev);
    }

    #[test]
    fn page_metadata_beyond_last() {
        let page: Page<String> = Page::new(vec![], 5, 99, 2);
        assert_eq!(page.total_pages, 3);
        assert!(!page.has_next);
        assert!(page.has_prev);
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
    fn cursor_page_with_more() {
        let page: CursorPage<String> =
            CursorPage::new(vec!["a".into(), "b".into()], Some("id_b".into()), 2);
        assert!(page.has_more);
        assert_eq!(page.next.as_deref(), Some("id_b"));
        assert_eq!(page.per_page, 2);
    }

    #[test]
    fn cursor_page_last_page() {
        let page: CursorPage<String> = CursorPage::new(vec!["a".into()], None, 20);
        assert!(!page.has_more);
        assert!(page.next.is_none());
    }

    #[test]
    fn cursor_page_serializes_to_json() {
        let page: CursorPage<i32> = CursorPage::new(vec![1, 2], Some("cursor_x".into()), 2);
        let json = serde_json::to_value(&page).unwrap();
        assert_eq!(json["items"], serde_json::json!([1, 2]));
        assert_eq!(json["next"], "cursor_x");
        assert_eq!(json["has_more"], true);
        assert_eq!(json["per_page"], 2);
    }
}
