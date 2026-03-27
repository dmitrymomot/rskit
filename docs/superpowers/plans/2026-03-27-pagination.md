# Pagination Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add offset-based and ID-keyset cursor pagination to modo as an always-available module (`src/page/`).

**Architecture:** Two independent builders (`Paginate` for offset, `CursorPaginate` for cursor) that collect SQL + owned argument values, execute queries against a `&impl Reader` pool, and return typed result containers (`Page<T>`, `CursorPage<T>`). Dedicated `FromRequestParts` extractors (`PageRequest`, `CursorRequest`) parse query params and clamp values using `PaginationConfig` from request extensions (with hardcoded fallback).

**Tech Stack:** sqlx 0.8 (SQLite), axum 0.8 extractors, serde serialization.

---

## File Structure

```
src/page/
  mod.rs         — pub mod + re-exports
  config.rs      — PaginationConfig (Deserialize, Default)
  value.rs       — SqliteValue enum + IntoSqliteValue trait (clonable arg storage)
  response.rs    — Page<T>, CursorPage<T> (Serialize)
  request.rs     — PageRequest, CursorRequest (FromRequestParts extractors)
  offset.rs      — Paginate builder
  cursor.rs      — CursorPaginate builder

Modified:
  src/lib.rs              — add `pub mod page;` + re-exports
  src/config/modo.rs      — add `pub pagination: PaginationConfig` field
  tests/page_test.rs      — integration tests (new file)
```

---

### Task 1: Module scaffolding, config, and wiring

**Files:**
- Create: `src/page/mod.rs`
- Create: `src/page/config.rs`
- Modify: `src/lib.rs`
- Modify: `src/config/modo.rs`

- [ ] **Step 1: Create `src/page/config.rs`**

```rust
use serde::Deserialize;

/// Pagination defaults applied by [`super::PageRequest`] and
/// [`super::CursorRequest`] extractors.
///
/// Loaded from the `pagination:` section of the YAML config. All fields
/// have sensible defaults, so the section can be omitted entirely.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PaginationConfig {
    /// Default number of items per page when `per_page` is not specified.
    pub default_per_page: u32,
    /// Maximum allowed value for `per_page`. Values above this are clamped.
    pub max_per_page: u32,
}

impl Default for PaginationConfig {
    fn default() -> Self {
        Self {
            default_per_page: 20,
            max_per_page: 100,
        }
    }
}
```

- [ ] **Step 2: Create `src/page/mod.rs`**

```rust
mod config;

pub use config::PaginationConfig;
```

- [ ] **Step 3: Add module declaration to `src/lib.rs`**

Add `pub mod page;` after line 37 (`pub mod ip;`), keeping alphabetical order among the always-available modules. Also add re-exports after line 94 (after the `validate` re-export):

```rust
pub use page::{CursorPage, CursorPaginate, CursorRequest, Page, Paginate, PageRequest, PaginationConfig};
```

Note: these re-exports will fail to compile initially since the types don't exist yet. That's fine — we'll add them incrementally and the re-exports will resolve as we create each type.

- [ ] **Step 4: Add pagination config to `src/config/modo.rs`**

Add after the `session` field (line 34):

```rust
    /// Pagination defaults (items per page, max page size).
    pub pagination: crate::page::PaginationConfig,
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check 2>&1 | head -20`

Expected: Compiles successfully (re-exports of not-yet-created types will be commented out or deferred — see Step 3 note). If there are errors about missing types in the re-export line, temporarily comment out the `pub use page::...` line in `lib.rs` until all types exist.

- [ ] **Step 6: Commit**

```
git add src/page/mod.rs src/page/config.rs src/lib.rs src/config/modo.rs
git commit -m "feat(page): scaffold module with PaginationConfig"
```

---

### Task 2: Clonable argument storage

The offset builder needs to clone arguments for its two-query pattern (COUNT + SELECT). `sqlx::sqlite::SqliteArguments` doesn't implement `Clone`, so we store bound values in our own `Clone`-able enum and convert to `SqliteArguments` at query time.

**Files:**
- Create: `src/page/value.rs`
- Modify: `src/page/mod.rs`

- [ ] **Step 1: Write unit tests for `SqliteValue` round-trip**

Add to the bottom of `src/page/value.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_converts_to_text() {
        let val = "hello".into_sqlite_value();
        assert!(matches!(val, SqliteValue::Text(ref s) if s == "hello"));
    }

    #[test]
    fn owned_string_converts_to_text() {
        let val = String::from("world").into_sqlite_value();
        assert!(matches!(val, SqliteValue::Text(ref s) if s == "world"));
    }

    #[test]
    fn i32_converts_to_int() {
        let val = 42i32.into_sqlite_value();
        assert!(matches!(val, SqliteValue::Int(42)));
    }

    #[test]
    fn i64_converts_to_int64() {
        let val = 100i64.into_sqlite_value();
        assert!(matches!(val, SqliteValue::Int64(100)));
    }

    #[test]
    fn f64_converts_to_double() {
        let val = 3.14f64.into_sqlite_value();
        assert!(matches!(val, SqliteValue::Double(v) if (v - 3.14).abs() < f64::EPSILON));
    }

    #[test]
    fn bool_converts() {
        let val = true.into_sqlite_value();
        assert!(matches!(val, SqliteValue::Bool(true)));
    }

    #[test]
    fn none_string_converts_to_null() {
        let val: Option<String> = None;
        let sv = val.into_sqlite_value();
        assert!(matches!(sv, SqliteValue::Null));
    }

    #[test]
    fn some_string_converts_to_text() {
        let val: Option<String> = Some("hi".into());
        let sv = val.into_sqlite_value();
        assert!(matches!(sv, SqliteValue::Text(ref s) if s == "hi"));
    }

    #[test]
    fn clone_preserves_value() {
        let val = "cloned".into_sqlite_value();
        let val2 = val.clone();
        assert!(matches!(val2, SqliteValue::Text(ref s) if s == "cloned"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib page::value -- 2>&1 | tail -5`

Expected: compilation errors (types don't exist yet).

- [ ] **Step 3: Implement `SqliteValue` and `IntoSqliteValue`**

Write `src/page/value.rs`:

```rust
use sqlx::Arguments;
use sqlx::sqlite::SqliteArguments;

/// Owned, cloneable representation of a single SQLite bind parameter.
///
/// The offset pagination builder needs to execute two queries (COUNT + SELECT)
/// with the same bind values. Because [`SqliteArguments`] does not implement
/// `Clone`, we store each bound value in this enum and convert to
/// `SqliteArguments` at query time.
#[derive(Clone, Debug)]
pub(crate) enum SqliteValue {
    Null,
    Bool(bool),
    Int(i32),
    Int64(i64),
    Double(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl SqliteValue {
    /// Push this value into a [`SqliteArguments`] buffer.
    pub(crate) fn add_to(self, args: &mut SqliteArguments<'_>) {
        // sqlx::Arguments::add returns Result — unwrap is safe for
        // these primitive types which always encode successfully.
        match self {
            Self::Null => args.add(Option::<String>::None).unwrap(),
            Self::Bool(v) => args.add(v).unwrap(),
            Self::Int(v) => args.add(v).unwrap(),
            Self::Int64(v) => args.add(v).unwrap(),
            Self::Double(v) => args.add(v).unwrap(),
            Self::Text(v) => args.add(v).unwrap(),
            Self::Blob(v) => args.add(v).unwrap(),
        }
    }
}

/// Convert a Rust value into a [`SqliteValue`] for deferred binding.
///
/// Implemented for the common types used in modo SQLite queries:
/// `bool`, `i32`, `i64`, `f64`, `String`, `&str`, `Vec<u8>`, `&[u8]`,
/// and `Option<T>` for any `T: IntoSqliteValue`.
pub trait IntoSqliteValue {
    /// Convert to an owned [`SqliteValue`].
    fn into_sqlite_value(self) -> SqliteValue;
}

impl IntoSqliteValue for bool {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Bool(self)
    }
}

impl IntoSqliteValue for i32 {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Int(self)
    }
}

impl IntoSqliteValue for i64 {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Int64(self)
    }
}

impl IntoSqliteValue for f64 {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Double(self)
    }
}

impl IntoSqliteValue for String {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Text(self)
    }
}

impl IntoSqliteValue for &str {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Text(self.to_owned())
    }
}

impl IntoSqliteValue for Vec<u8> {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Blob(self)
    }
}

impl IntoSqliteValue for &[u8] {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Blob(self.to_vec())
    }
}

impl<T: IntoSqliteValue> IntoSqliteValue for Option<T> {
    fn into_sqlite_value(self) -> SqliteValue {
        match self {
            Some(v) => v.into_sqlite_value(),
            None => SqliteValue::Null,
        }
    }
}

/// Build a [`SqliteArguments`] from a slice of [`SqliteValue`]s.
pub(crate) fn build_args(values: &[SqliteValue]) -> SqliteArguments<'static> {
    let mut args = SqliteArguments::default();
    for v in values {
        v.clone().add_to(&mut args);
    }
    args
}
```

- [ ] **Step 4: Add `value` module to `src/page/mod.rs`**

```rust
mod config;
mod value;

pub use config::PaginationConfig;

pub(crate) use value::{IntoSqliteValue, SqliteValue, build_args};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib page::value 2>&1 | tail -10`

Expected: all tests pass.

- [ ] **Step 6: Commit**

```
git add src/page/value.rs src/page/mod.rs
git commit -m "feat(page): add SqliteValue for clonable bind args"
```

---

### Task 3: Response types

**Files:**
- Create: `src/page/response.rs`
- Modify: `src/page/mod.rs`

- [ ] **Step 1: Write unit tests for `Page<T>` metadata computation**

Add to the bottom of `src/page/response.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib page::response -- 2>&1 | tail -5`

Expected: compilation errors.

- [ ] **Step 3: Implement response types**

Write `src/page/response.rs`:

```rust
use serde::Serialize;

/// Offset-paginated result set.
///
/// Contains the items for the current page plus metadata for navigating
/// through the full result set. Pages are **1-based**.
///
/// Constructed by [`super::Paginate::fetch`] or manually via [`Page::new`].
#[derive(Debug, Clone, Serialize)]
pub struct Page<T: Serialize> {
    /// Items on the current page.
    pub items: Vec<T>,
    /// Total number of matching rows across all pages.
    pub total: u64,
    /// Current page number (1-based).
    pub page: u32,
    /// Number of items per page.
    pub per_page: u32,
    /// Total number of pages: `ceil(total / per_page)`.
    pub total_pages: u32,
    /// Whether a next page exists.
    pub has_next: bool,
    /// Whether a previous page exists.
    pub has_prev: bool,
}

impl<T: Serialize> Page<T> {
    /// Build a `Page` from items, total count, current page, and page size.
    ///
    /// Computes `total_pages`, `has_next`, and `has_prev` automatically.
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
/// Contains the items for the current page, an optional cursor pointing to
/// the next page, and a flag indicating whether more items exist.
///
/// Constructed by [`super::CursorPaginate::fetch`] or manually via
/// [`CursorPage::new`].
#[derive(Debug, Clone, Serialize)]
pub struct CursorPage<T: Serialize> {
    /// Items on the current page.
    pub items: Vec<T>,
    /// The ID of the last item — pass as `after` to fetch the next page.
    /// `None` when there are no more pages.
    pub next: Option<String>,
    /// Whether more items exist beyond this page.
    pub has_more: bool,
    /// Number of items per page.
    pub per_page: u32,
}

impl<T: Serialize> CursorPage<T> {
    /// Build a `CursorPage` from items, an optional next-cursor, and page size.
    ///
    /// `has_more` is `true` when `next` is `Some`.
    pub fn new(items: Vec<T>, next: Option<String>, per_page: u32) -> Self {
        Self {
            has_more: next.is_some(),
            items,
            next,
            per_page,
        }
    }
}
```

- [ ] **Step 4: Add `response` to `src/page/mod.rs`**

Update `src/page/mod.rs`:

```rust
mod config;
mod response;
mod value;

pub use config::PaginationConfig;
pub use response::{CursorPage, Page};

pub(crate) use value::{IntoSqliteValue, SqliteValue, build_args};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib page::response 2>&1 | tail -15`

Expected: all tests pass.

- [ ] **Step 6: Commit**

```
git add src/page/response.rs src/page/mod.rs
git commit -m "feat(page): add Page<T> and CursorPage<T> response types"
```

---

### Task 4: Request extractors

**Files:**
- Create: `src/page/request.rs`
- Modify: `src/page/mod.rs`

- [ ] **Step 1: Write unit tests for clamping logic**

Add to the bottom of `src/page/request.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> PaginationConfig {
        PaginationConfig {
            default_per_page: 20,
            max_per_page: 100,
        }
    }

    // --- PageRequest clamping ---

    #[test]
    fn page_request_defaults() {
        let req = PageRequest::default();
        let req = req.clamp(&config());
        assert_eq!(req.page, 1);
        assert_eq!(req.per_page, 20);
    }

    #[test]
    fn page_request_zero_page_becomes_one() {
        let req = PageRequest { page: 0, per_page: 10 };
        let req = req.clamp(&config());
        assert_eq!(req.page, 1);
    }

    #[test]
    fn page_request_per_page_zero_uses_default() {
        let req = PageRequest { page: 1, per_page: 0 };
        let req = req.clamp(&config());
        assert_eq!(req.per_page, 20); // config default, not 1
    }

    #[test]
    fn page_request_per_page_over_max_clamped() {
        let req = PageRequest { page: 1, per_page: 999 };
        let req = req.clamp(&config());
        assert_eq!(req.per_page, 100);
    }

    #[test]
    fn page_request_valid_values_unchanged() {
        let req = PageRequest { page: 3, per_page: 50 };
        let req = req.clamp(&config());
        assert_eq!(req.page, 3);
        assert_eq!(req.per_page, 50);
    }

    #[test]
    fn page_request_offset_calculation() {
        let req = PageRequest { page: 3, per_page: 10 };
        assert_eq!(req.offset(), 20);
    }

    #[test]
    fn page_request_offset_first_page() {
        let req = PageRequest { page: 1, per_page: 10 };
        assert_eq!(req.offset(), 0);
    }

    // --- CursorRequest clamping ---

    #[test]
    fn cursor_request_defaults() {
        let req = CursorRequest::default();
        let req = req.clamp(&config());
        assert!(req.after.is_none());
        assert_eq!(req.per_page, 20);
    }

    #[test]
    fn cursor_request_per_page_over_max_clamped() {
        let req = CursorRequest { after: None, per_page: 500 };
        let req = req.clamp(&config());
        assert_eq!(req.per_page, 100);
    }

    #[test]
    fn cursor_request_per_page_zero_becomes_default() {
        let req = CursorRequest { after: Some("abc".into()), per_page: 0 };
        let req = req.clamp(&config());
        assert_eq!(req.per_page, 20);
        assert_eq!(req.after.as_deref(), Some("abc"));
    }

    // --- Deserialization ---

    #[test]
    fn page_request_deserializes_from_query_string() {
        let req: PageRequest =
            serde_urlencoded::from_str("page=2&per_page=30").unwrap();
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
        let req: CursorRequest =
            serde_urlencoded::from_str("after=01ABC&per_page=10").unwrap();
        assert_eq!(req.after.as_deref(), Some("01ABC"));
        assert_eq!(req.per_page, 10);
    }

    #[test]
    fn cursor_request_deserializes_without_after() {
        let req: CursorRequest =
            serde_urlencoded::from_str("per_page=10").unwrap();
        assert!(req.after.is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib page::request -- 2>&1 | tail -5`

Expected: compilation errors.

- [ ] **Step 3: Implement extractors**

Write `src/page/request.rs`:

```rust
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
    /// Current page number (1-based). Clamped to a minimum of 1.
    #[serde(default)]
    pub page: u32,
    /// Number of items per page. Clamped to `[1, max_per_page]`.
    #[serde(default)]
    pub per_page: u32,
}

/// Cursor pagination parameters extracted from the query string.
///
/// Parsed from `?after=01JQDKV...&per_page=20`. Implements
/// [`FromRequestParts`] so it can be used directly as a handler argument.
/// `after` is the ID of the last item from the previous page; omit it to
/// fetch the first page.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CursorRequest {
    /// ID of the last item from the previous page. `None` means first page.
    #[serde(default)]
    pub after: Option<String>,
    /// Number of items per page. Clamped to `[1, max_per_page]`.
    #[serde(default)]
    pub per_page: u32,
}

impl PageRequest {
    /// Apply clamping rules using the given config.
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
    /// Apply clamping rules using the given config.
    pub(crate) fn clamp(mut self, config: &PaginationConfig) -> Self {
        if self.per_page == 0 {
            self.per_page = config.default_per_page;
        }
        self.per_page = self.per_page.clamp(1, config.max_per_page);
        self
    }
}

/// Resolve config from request extensions with hardcoded fallback.
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
```

- [ ] **Step 4: Add `request` to `src/page/mod.rs`**

Update `src/page/mod.rs`:

```rust
mod config;
mod request;
mod response;
mod value;

pub use config::PaginationConfig;
pub use request::{CursorRequest, PageRequest};
pub use response::{CursorPage, Page};

pub(crate) use value::{IntoSqliteValue, SqliteValue, build_args};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib page::request 2>&1 | tail -15`

Expected: all tests pass.

- [ ] **Step 6: Commit**

```
git add src/page/request.rs src/page/mod.rs
git commit -m "feat(page): add PageRequest and CursorRequest extractors"
```

---

### Task 5: Offset pagination builder

**Files:**
- Create: `src/page/offset.rs`
- Modify: `src/page/mod.rs`

- [ ] **Step 1: Write unit test for SQL generation**

Add to the bottom of `src/page/offset.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib page::offset -- 2>&1 | tail -5`

Expected: compilation errors.

- [ ] **Step 3: Implement `Paginate`**

Write `src/page/offset.rs`:

```rust
use sqlx::sqlite::SqliteRow;
use sqlx::FromRow;
use serde::Serialize;

use crate::db::Reader;
use crate::error::Error;

use super::request::PageRequest;
use super::response::Page;
use super::value::{IntoSqliteValue, SqliteValue, build_args};

/// Builder for offset-based pagination queries.
///
/// Collects a base SQL string and bind parameters, then executes a COUNT
/// query and a data query against a [`Reader`] pool.
///
/// # Example
///
/// ```rust,no_run
/// # async fn example(pool: &impl modo::db::Reader, req: &modo::page::PageRequest) -> modo::Result<()> {
/// use modo::page::Paginate;
///
/// let page = Paginate::new("SELECT * FROM users WHERE active = ?")
///     .bind(true)
///     .fetch::<modo::serde_json::Value>(&pool, &req)
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct Paginate {
    sql: String,
    args: Vec<SqliteValue>,
    where_sql: Option<String>,
    where_args: Vec<SqliteValue>,
}

impl Paginate {
    /// Create a new builder with the given base SQL query.
    ///
    /// The SQL should be a complete SELECT statement **without** LIMIT/OFFSET —
    /// those are appended automatically by [`fetch`](Self::fetch).
    pub fn new(sql: &str) -> Self {
        Self {
            sql: sql.to_owned(),
            args: Vec::new(),
            where_sql: None,
            where_args: Vec::new(),
        }
    }

    /// Bind a parameter to the base SQL query.
    ///
    /// Parameters are bound in order, matching `?` placeholders in the SQL.
    pub fn bind(mut self, value: impl IntoSqliteValue) -> Self {
        self.args.push(value.into_sqlite_value());
        self
    }

    /// Append a WHERE clause fragment (for future filter module integration).
    ///
    /// The fragment is inserted between the base SQL and LIMIT/OFFSET. Its
    /// args are bound after the base SQL args.
    pub fn where_clause(mut self, sql: &str, args: Vec<SqliteValue>) -> Self {
        self.where_sql = Some(sql.to_owned());
        self.where_args = args;
        self
    }

    /// Execute the COUNT and data queries, returning a [`Page<T>`].
    ///
    /// Runs two queries against the pool:
    /// 1. `SELECT COUNT(*) FROM ({base_sql}{where_clause})` — to get total
    /// 2. `{base_sql}{where_clause} LIMIT ? OFFSET ?` — to get items
    ///
    /// If the requested page exceeds `total_pages`, returns an empty `items`
    /// vec with correct metadata.
    pub async fn fetch<T>(
        &self,
        pool: &(impl Reader + Sync),
        req: &PageRequest,
    ) -> crate::Result<Page<T>>
    where
        T: for<'r> FromRow<'r, SqliteRow> + Serialize + Send + Unpin,
    {
        // --- Count query ---
        let count_sql = self.count_sql();
        let count_args = self.all_args();
        let (total,): (i64,) = sqlx::query_as_with(&count_sql, count_args)
            .fetch_one(pool.read_pool())
            .await
            .map_err(|e| Error::internal("pagination count query failed").chain(e))?;
        let total = total.max(0) as u64;

        // --- Data query ---
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

    /// Combine base args + where args into a single `SqliteArguments`.
    fn all_args(&self) -> sqlx::sqlite::SqliteArguments<'static> {
        let combined: Vec<SqliteValue> = self
            .args
            .iter()
            .chain(self.where_args.iter())
            .cloned()
            .collect();
        build_args(&combined)
    }

    /// Build the COUNT SQL string.
    pub(crate) fn count_sql(&self) -> String {
        let where_part = self.where_sql.as_deref().unwrap_or("");
        format!("SELECT COUNT(*) FROM ({}{})", self.sql, where_part)
    }

    /// Build the data SQL string with LIMIT/OFFSET placeholders.
    pub(crate) fn data_sql(&self) -> String {
        let where_part = self.where_sql.as_deref().unwrap_or("");
        format!("{}{} LIMIT ? OFFSET ?", self.sql, where_part)
    }
}
```

- [ ] **Step 4: Add `offset` to `src/page/mod.rs`**

Update `src/page/mod.rs`:

```rust
mod config;
mod offset;
mod request;
mod response;
mod value;

pub use config::PaginationConfig;
pub use offset::Paginate;
pub use request::{CursorRequest, PageRequest};
pub use response::{CursorPage, Page};

pub(crate) use value::{IntoSqliteValue, SqliteValue, build_args};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib page::offset 2>&1 | tail -10`

Expected: all tests pass.

- [ ] **Step 6: Commit**

```
git add src/page/offset.rs src/page/mod.rs
git commit -m "feat(page): add Paginate offset pagination builder"
```

---

### Task 6: Cursor pagination builder

**Files:**
- Create: `src/page/cursor.rs`
- Modify: `src/page/mod.rs`

- [ ] **Step 1: Write unit tests for SQL generation**

Add to the bottom of `src/page/cursor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::value::IntoSqliteValue;

    #[test]
    fn first_page_sql_newest_first() {
        let p = CursorPaginate::new("SELECT * FROM events");
        let req = CursorRequest { after: None, per_page: 20 };
        let (sql, limit) = p.build_query(&req);
        assert_eq!(sql, "SELECT * FROM (SELECT * FROM events) ORDER BY id DESC LIMIT ?");
        assert_eq!(limit, 21);
    }

    #[test]
    fn next_page_sql_newest_first() {
        let p = CursorPaginate::new("SELECT * FROM events");
        let req = CursorRequest { after: Some("abc".into()), per_page: 20 };
        let (sql, _) = p.build_query(&req);
        assert_eq!(sql, "SELECT * FROM (SELECT * FROM events) WHERE id < ? ORDER BY id DESC LIMIT ?");
    }

    #[test]
    fn first_page_sql_oldest_first() {
        let p = CursorPaginate::new("SELECT * FROM events").oldest_first();
        let req = CursorRequest { after: None, per_page: 10 };
        let (sql, _) = p.build_query(&req);
        assert_eq!(sql, "SELECT * FROM (SELECT * FROM events) ORDER BY id ASC LIMIT ?");
    }

    #[test]
    fn next_page_sql_oldest_first() {
        let p = CursorPaginate::new("SELECT * FROM events").oldest_first();
        let req = CursorRequest { after: Some("abc".into()), per_page: 10 };
        let (sql, _) = p.build_query(&req);
        assert_eq!(sql, "SELECT * FROM (SELECT * FROM events) WHERE id > ? ORDER BY id ASC LIMIT ?");
    }

    #[test]
    fn where_clause_included() {
        let p = CursorPaginate::new("SELECT * FROM events")
            .where_clause(" WHERE tenant_id = ?", vec!["t1".into_sqlite_value()]);
        let req = CursorRequest { after: None, per_page: 5 };
        let (sql, _) = p.build_query(&req);
        assert_eq!(
            sql,
            "SELECT * FROM (SELECT * FROM events WHERE tenant_id = ?) ORDER BY id DESC LIMIT ?"
        );
    }

    #[test]
    fn limit_is_per_page_plus_one() {
        let p = CursorPaginate::new("SELECT * FROM events");
        let req = CursorRequest { after: None, per_page: 5 };
        let (_, limit) = p.build_query(&req);
        assert_eq!(limit, 6);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib page::cursor -- 2>&1 | tail -5`

Expected: compilation errors.

- [ ] **Step 3: Implement `CursorPaginate`**

Write `src/page/cursor.rs`:

```rust
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use sqlx::FromRow;
use serde::Serialize;

use crate::db::Reader;
use crate::error::Error;

use super::request::CursorRequest;
use super::response::CursorPage;
use super::value::{IntoSqliteValue, SqliteValue, build_args};

/// Builder for ID-based keyset (cursor) pagination queries.
///
/// Uses the `id` column for cursor positioning. Returns newest items first
/// by default; call [`.oldest_first()`](Self::oldest_first) to reverse.
///
/// # Example
///
/// ```rust,no_run
/// # async fn example(pool: &impl modo::db::Reader, req: &modo::page::CursorRequest) -> modo::Result<()> {
/// use modo::page::CursorPaginate;
///
/// let page = CursorPaginate::new("SELECT * FROM events WHERE tenant_id = ?")
///     .bind("tenant_abc")
///     .fetch::<modo::serde_json::Value>(&pool, &req)
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct CursorPaginate {
    sql: String,
    args: Vec<SqliteValue>,
    oldest_first: bool,
    where_sql: Option<String>,
    where_args: Vec<SqliteValue>,
}

impl CursorPaginate {
    /// Create a new builder with the given base SQL query.
    ///
    /// The SQL should be a complete SELECT that includes an `id` column.
    /// Do **not** add ORDER BY or LIMIT — those are appended automatically.
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

    /// Sort results oldest-first (`ORDER BY id ASC`).
    ///
    /// By default, results are newest-first (`ORDER BY id DESC`).
    pub fn oldest_first(mut self) -> Self {
        self.oldest_first = true;
        self
    }

    /// Append a WHERE clause fragment (for future filter module integration).
    pub fn where_clause(mut self, sql: &str, args: Vec<SqliteValue>) -> Self {
        self.where_sql = Some(sql.to_owned());
        self.where_args = args;
        self
    }

    /// Execute the cursor query and return a [`CursorPage<T>`].
    ///
    /// Fetches `per_page + 1` rows to detect whether more items exist.
    /// The `id` column is read from the last returned row to produce the
    /// `next` cursor.
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
            format!(
                "SELECT * FROM ({inner}) WHERE id {op} ? ORDER BY id {order} LIMIT ?"
            )
        } else {
            format!("SELECT * FROM ({inner}) ORDER BY id {order} LIMIT ?")
        };

        (sql, limit)
    }

    /// Build `SqliteArguments` for the query.
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
```

- [ ] **Step 4: Add `cursor` to `src/page/mod.rs`**

Update `src/page/mod.rs`:

```rust
mod config;
mod cursor;
mod offset;
mod request;
mod response;
mod value;

pub use config::PaginationConfig;
pub use cursor::CursorPaginate;
pub use offset::Paginate;
pub use request::{CursorRequest, PageRequest};
pub use response::{CursorPage, Page};

pub(crate) use value::{IntoSqliteValue, SqliteValue, build_args};
```

- [ ] **Step 5: Uncomment re-exports in `src/lib.rs`**

Now that all types exist, ensure the re-export line in `src/lib.rs` is active:

```rust
pub use page::{CursorPage, CursorPaginate, CursorRequest, Page, Paginate, PageRequest, PaginationConfig};
```

- [ ] **Step 6: Run tests and check compilation**

Run: `cargo test --lib page:: 2>&1 | tail -20`

Expected: all unit tests pass, project compiles.

- [ ] **Step 7: Commit**

```
git add src/page/cursor.rs src/page/mod.rs src/lib.rs
git commit -m "feat(page): add CursorPaginate cursor pagination builder"
```

---

### Task 7: Integration tests

**Files:**
- Create: `tests/page_test.rs`

- [ ] **Step 1: Write integration tests**

Create `tests/page_test.rs`:

```rust
#![cfg(feature = "test-helpers")]

use axum::routing::get;
use modo::page::{
    CursorPage, CursorPaginate, CursorRequest, Page, Paginate, PageRequest, PaginationConfig,
};
use modo::testing::{TestApp, TestDb};

// --- Helpers ---

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
struct Item {
    id: String,
    name: String,
}

async fn setup_db() -> TestDb {
    TestDb::new()
        .await
        .exec(
            "CREATE TABLE items (
                id   TEXT PRIMARY KEY,
                name TEXT NOT NULL
            )",
        )
        .await
}

async fn seed_items(db: &TestDb, count: usize) -> Vec<String> {
    let pool = db.pool();
    let mut ids = Vec::new();
    for i in 1..=count {
        // Zero-padded numbers to simulate ULID lexicographic ordering.
        let id = format!("{i:026}");
        let name = format!("item_{i}");
        sqlx::query("INSERT INTO items (id, name) VALUES (?, ?)")
            .bind(&id)
            .bind(&name)
            .execute(&*pool)
            .await
            .unwrap();
        ids.push(id);
    }
    ids
}

// --- Offset pagination ---

#[tokio::test]
async fn offset_first_page() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    let req = PageRequest { page: 1, per_page: 2 };
    let page: Page<Item> = Paginate::new("SELECT * FROM items ORDER BY id ASC")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id, ids[0]);
    assert_eq!(page.items[1].id, ids[1]);
    assert_eq!(page.total, 5);
    assert_eq!(page.page, 1);
    assert_eq!(page.per_page, 2);
    assert_eq!(page.total_pages, 3);
    assert!(page.has_next);
    assert!(!page.has_prev);
}

#[tokio::test]
async fn offset_middle_page() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    let req = PageRequest { page: 2, per_page: 2 };
    let page: Page<Item> = Paginate::new("SELECT * FROM items ORDER BY id ASC")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id, ids[2]);
    assert_eq!(page.items[1].id, ids[3]);
    assert!(page.has_next);
    assert!(page.has_prev);
}

#[tokio::test]
async fn offset_last_page() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    let req = PageRequest { page: 3, per_page: 2 };
    let page: Page<Item> = Paginate::new("SELECT * FROM items ORDER BY id ASC")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, ids[4]);
    assert!(!page.has_next);
    assert!(page.has_prev);
}

#[tokio::test]
async fn offset_beyond_last_page() {
    let db = setup_db().await;
    seed_items(&db, 3).await;
    let pool = db.read_pool();

    let req = PageRequest { page: 99, per_page: 2 };
    let page: Page<Item> = Paginate::new("SELECT * FROM items ORDER BY id ASC")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert!(page.items.is_empty());
    assert_eq!(page.total, 3);
    assert_eq!(page.total_pages, 2);
}

#[tokio::test]
async fn offset_empty_table() {
    let db = setup_db().await;
    let pool = db.read_pool();

    let req = PageRequest { page: 1, per_page: 20 };
    let page: Page<Item> = Paginate::new("SELECT * FROM items")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert!(page.items.is_empty());
    assert_eq!(page.total, 0);
    assert_eq!(page.total_pages, 0);
    assert!(!page.has_next);
    assert!(!page.has_prev);
}

#[tokio::test]
async fn offset_with_bind_params() {
    let db = setup_db().await;
    seed_items(&db, 5).await;
    sqlx::query("INSERT INTO items (id, name) VALUES (?, ?)")
        .bind("special_001")
        .bind("special")
        .execute(&*db.pool())
        .await
        .unwrap();
    let pool = db.read_pool();

    let req = PageRequest { page: 1, per_page: 10 };
    let page: Page<Item> = Paginate::new("SELECT * FROM items WHERE name = ?")
        .bind("special")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.total, 1);
}

// --- Cursor pagination ---

#[tokio::test]
async fn cursor_first_page_newest_first() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    let req = CursorRequest { after: None, per_page: 2 };
    let page: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 2);
    // Newest first = descending by id
    assert_eq!(page.items[0].id, ids[4]);
    assert_eq!(page.items[1].id, ids[3]);
    assert!(page.has_more);
    assert_eq!(page.next.as_deref(), Some(ids[3].as_str()));
}

#[tokio::test]
async fn cursor_second_page_newest_first() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    // First page
    let req = CursorRequest { after: None, per_page: 2 };
    let page1: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .fetch(&pool, &req)
        .await
        .unwrap();

    // Second page using cursor from first
    let req2 = CursorRequest {
        after: page1.next.clone(),
        per_page: 2,
    };
    let page2: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .fetch(&pool, &req2)
        .await
        .unwrap();

    assert_eq!(page2.items.len(), 2);
    assert_eq!(page2.items[0].id, ids[2]);
    assert_eq!(page2.items[1].id, ids[1]);
    assert!(page2.has_more);
}

#[tokio::test]
async fn cursor_last_page() {
    let db = setup_db().await;
    let ids = seed_items(&db, 3).await;
    let pool = db.read_pool();

    // Request starting after the second item (should get only the first)
    let req = CursorRequest {
        after: Some(ids[1].clone()),
        per_page: 10,
    };
    let page: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, ids[0]);
    assert!(!page.has_more);
    assert!(page.next.is_none());
}

#[tokio::test]
async fn cursor_empty_table() {
    let db = setup_db().await;
    let pool = db.read_pool();

    let req = CursorRequest { after: None, per_page: 20 };
    let page: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert!(page.items.is_empty());
    assert!(!page.has_more);
    assert!(page.next.is_none());
}

#[tokio::test]
async fn cursor_oldest_first() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    let req = CursorRequest { after: None, per_page: 2 };
    let page: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .oldest_first()
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id, ids[0]);
    assert_eq!(page.items[1].id, ids[1]);
    assert!(page.has_more);
    assert_eq!(page.next.as_deref(), Some(ids[1].as_str()));
}

#[tokio::test]
async fn cursor_oldest_first_second_page() {
    let db = setup_db().await;
    let ids = seed_items(&db, 5).await;
    let pool = db.read_pool();

    let req = CursorRequest {
        after: Some(ids[1].clone()),
        per_page: 2,
    };
    let page: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items")
        .oldest_first()
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id, ids[2]);
    assert_eq!(page.items[1].id, ids[3]);
    assert!(page.has_more);
}

#[tokio::test]
async fn cursor_with_bind_params() {
    let db = setup_db().await;
    seed_items(&db, 5).await;
    sqlx::query("INSERT INTO items (id, name) VALUES (?, ?)")
        .bind("z_special_001")
        .bind("special")
        .execute(&*db.pool())
        .await
        .unwrap();
    let pool = db.read_pool();

    let req = CursorRequest { after: None, per_page: 10 };
    let page: CursorPage<Item> = CursorPaginate::new("SELECT * FROM items WHERE name = ?")
        .bind("special")
        .fetch(&pool, &req)
        .await
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].name, "special");
}

// --- Extractor integration ---

async fn list_items_offset(
    page_req: PageRequest,
    modo::extractor::Service(pool): modo::extractor::Service<modo::db::ReadPool>,
) -> modo::Result<axum::Json<Page<Item>>> {
    let page = Paginate::new("SELECT * FROM items ORDER BY id ASC")
        .fetch(&pool, &page_req)
        .await?;
    Ok(axum::Json(page))
}

async fn list_items_cursor(
    cursor_req: CursorRequest,
    modo::extractor::Service(pool): modo::extractor::Service<modo::db::ReadPool>,
) -> modo::Result<axum::Json<CursorPage<Item>>> {
    let page = CursorPaginate::new("SELECT * FROM items")
        .fetch(&pool, &cursor_req)
        .await?;
    Ok(axum::Json(page))
}

#[tokio::test]
async fn extractor_offset_default_params() {
    let db = setup_db().await;
    seed_items(&db, 25).await;

    let app = TestApp::builder()
        .service(db.read_pool())
        .route("/items", get(list_items_offset))
        .build();

    let res = app.get("/items").send().await;
    assert_eq!(res.status(), 200);

    let page: Page<Item> = res.json();
    // Default per_page = 20 (hardcoded fallback)
    assert_eq!(page.items.len(), 20);
    assert_eq!(page.per_page, 20);
    assert_eq!(page.page, 1);
    assert_eq!(page.total, 25);
}

#[tokio::test]
async fn extractor_offset_custom_params() {
    let db = setup_db().await;
    seed_items(&db, 10).await;

    let app = TestApp::builder()
        .service(db.read_pool())
        .route("/items", get(list_items_offset))
        .build();

    let res = app.get("/items?page=2&per_page=3").send().await;
    assert_eq!(res.status(), 200);

    let page: Page<Item> = res.json();
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.page, 2);
    assert_eq!(page.per_page, 3);
}

#[tokio::test]
async fn extractor_offset_per_page_clamped_to_max() {
    let db = setup_db().await;
    seed_items(&db, 5).await;

    let app = TestApp::builder()
        .service(db.read_pool())
        .route("/items", get(list_items_offset))
        .build();

    // Default max is 100, request 999
    let res = app.get("/items?per_page=999").send().await;
    let page: Page<Item> = res.json();
    assert_eq!(page.per_page, 100);
}

#[tokio::test]
async fn extractor_cursor_first_and_next_page() {
    let db = setup_db().await;
    seed_items(&db, 5).await;

    let app = TestApp::builder()
        .service(db.read_pool())
        .route("/items", get(list_items_cursor))
        .build();

    // First page
    let res = app.get("/items?per_page=2").send().await;
    assert_eq!(res.status(), 200);
    let page1: CursorPage<Item> = res.json();
    assert_eq!(page1.items.len(), 2);
    assert!(page1.has_more);

    // Second page
    let next = page1.next.unwrap();
    let res = app
        .get(&format!("/items?after={next}&per_page=2"))
        .send()
        .await;
    let page2: CursorPage<Item> = res.json();
    assert_eq!(page2.items.len(), 2);
    assert!(page2.has_more);

    // No overlap between pages
    let ids1: Vec<_> = page1.items.iter().map(|i| &i.id).collect();
    let ids2: Vec<_> = page2.items.iter().map(|i| &i.id).collect();
    assert!(ids1.iter().all(|id| !ids2.contains(id)));
}

#[tokio::test]
async fn extractor_with_config_from_extensions() {
    let db = setup_db().await;
    seed_items(&db, 50).await;

    let config = PaginationConfig {
        default_per_page: 5,
        max_per_page: 10,
    };

    // Inject config via a middleware layer that adds to extensions
    let config_layer = axum::middleware::from_fn(
        move |mut req: axum::extract::Request, next: axum::middleware::Next| {
            let cfg = config.clone();
            async move {
                req.extensions_mut().insert(cfg);
                next.run(req).await
            }
        },
    );

    let app = TestApp::builder()
        .service(db.read_pool())
        .route("/items", get(list_items_offset))
        .layer(config_layer)
        .build();

    // No per_page specified — should use config default of 5
    let res = app.get("/items").send().await;
    let page: Page<Item> = res.json();
    assert_eq!(page.per_page, 5);

    // per_page=50 should be clamped to config max of 10
    let res = app.get("/items?per_page=50").send().await;
    let page: Page<Item> = res.json();
    assert_eq!(page.per_page, 10);
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --features test-helpers --test page_test 2>&1 | tail -30`

Expected: all tests pass.

- [ ] **Step 3: Run full test suite to check for regressions**

Run: `cargo test --features test-helpers 2>&1 | tail -10`

Expected: all existing tests still pass.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --tests -- -D warnings 2>&1 | tail -10`

Expected: no warnings.

- [ ] **Step 5: Commit**

```
git add tests/page_test.rs
git commit -m "test(page): add integration tests for pagination module"
```

---

### Task 8: Final verification and cleanup

- [ ] **Step 1: Run the full test suite with all features**

Run: `cargo test --all-features 2>&1 | tail -15`

Expected: all tests pass.

- [ ] **Step 2: Run clippy with all features**

Run: `cargo clippy --all-features --tests -- -D warnings 2>&1 | tail -10`

Expected: no warnings.

- [ ] **Step 3: Run cargo fmt check**

Run: `cargo fmt --check 2>&1`

Expected: no formatting issues (or fix with `cargo fmt`).

- [ ] **Step 4: Verify re-exports are all accessible**

Run a quick compilation check that uses the public API:

```
cargo check --features test-helpers 2>&1 | tail -5
```

Expected: compiles cleanly.

- [ ] **Step 5: Commit any cleanup**

If any formatting or cleanup was needed:

```
git add -A
git commit -m "chore(page): formatting and cleanup"
```
