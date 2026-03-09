use sea_orm::{ConnectionTrait, DbErr, EntityTrait, FromQueryResult, Select};
use sea_orm::{IntoIdentity, QuerySelect};
use serde::{Deserialize, Serialize};

// ── Offset / Page Pagination ────────────────────────────────────────────────

/// Query-string parameters for offset-based pagination.
///
/// Page is 1-indexed (default 1). `per_page` is clamped to `[1, 100]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PageParams {
    pub page: u64,
    pub per_page: u64,
}

impl Default for PageParams {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
        }
    }
}

impl PageParams {
    fn clamped(&self) -> (u64, u64) {
        (self.page.max(1), self.per_page.clamp(1, 100))
    }
}

/// Paginated response for offset-based pagination.
#[derive(Debug, Clone, Serialize)]
pub struct PageResult<T> {
    pub data: Vec<T>,
    pub page: u64,
    pub per_page: u64,
    pub has_next: bool,
    pub has_prev: bool,
}

impl<T> PageResult<T> {
    /// Transform every item in `data` via `f`.
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> PageResult<U> {
        PageResult {
            data: self.data.into_iter().map(f).collect(),
            page: self.page,
            per_page: self.per_page,
            has_next: self.has_next,
            has_prev: self.has_prev,
        }
    }
}

/// Run an offset-based paginated query.
///
/// Uses the **limit + 1** trick to detect `has_next` without a COUNT query.
pub async fn paginate<E, M>(
    query: Select<E>,
    db: &impl ConnectionTrait,
    params: &PageParams,
) -> Result<PageResult<M>, DbErr>
where
    E: EntityTrait<Model = M>,
    M: FromQueryResult + Sized + Send + Sync,
{
    let (page, per_page) = params.clamped();
    let offset = (page - 1) * per_page;

    let mut rows = query.offset(offset).limit(per_page + 1).all(db).await?;

    let has_next = rows.len() as u64 > per_page;
    if has_next {
        rows.truncate(per_page as usize);
    }

    Ok(PageResult {
        data: rows,
        page,
        per_page,
        has_next,
        has_prev: page > 1,
    })
}

// ── Cursor Pagination ───────────────────────────────────────────────────────

/// Query-string parameters for cursor-based pagination.
///
/// `per_page` is clamped to `[1, 100]`. If both `after` and `before` are set,
/// `after` takes precedence.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CursorParams {
    pub per_page: Option<u64>,
    pub after: Option<String>,
    pub before: Option<String>,
}

impl CursorParams {
    fn clamped_per_page(&self) -> u64 {
        self.per_page.unwrap_or(20).clamp(1, 100)
    }
}

/// Paginated response for cursor-based pagination.
#[derive(Debug, Clone, Serialize)]
pub struct CursorResult<T> {
    pub data: Vec<T>,
    pub per_page: u64,
    pub next_cursor: Option<String>,
    pub prev_cursor: Option<String>,
    pub has_next: bool,
    pub has_prev: bool,
}

impl<T> CursorResult<T> {
    /// Transform every item in `data` via `f`.
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> CursorResult<U> {
        CursorResult {
            data: self.data.into_iter().map(f).collect(),
            per_page: self.per_page,
            next_cursor: self.next_cursor,
            prev_cursor: self.prev_cursor,
            has_next: self.has_next,
            has_prev: self.has_prev,
        }
    }
}

/// Run a cursor-based paginated query.
///
/// Uses SeaORM's [`Cursor`] with the **limit + 1** trick.
///
/// - `cursor_column` — the column to paginate on (e.g. `Column::Id`).
/// - `cursor_fn` — extracts the cursor string from a model instance.
pub async fn paginate_cursor<E, M, C, F>(
    query: Select<E>,
    cursor_column: C,
    cursor_fn: F,
    db: &impl ConnectionTrait,
    params: &CursorParams,
) -> Result<CursorResult<M>, DbErr>
where
    E: EntityTrait<Model = M>,
    M: FromQueryResult + Sized + Send + Sync,
    C: IntoIdentity,
    F: Fn(&M) -> String,
{
    let per_page = params.clamped_per_page();
    let mut cursor = query.cursor_by(cursor_column);

    // `after` wins when both are set.
    let is_backward = params.after.is_none() && params.before.is_some();

    if let Some(ref after) = params.after {
        cursor.after(after.clone());
        cursor.first(per_page + 1);
    } else if let Some(ref before) = params.before {
        cursor.before(before.clone());
        cursor.last(per_page + 1);
    } else {
        cursor.first(per_page + 1);
    }

    let mut rows = cursor.all(db).await?;

    let (has_next, has_prev);

    if is_backward {
        has_prev = rows.len() as u64 > per_page;
        has_next = true; // we navigated backward from a later page
        if has_prev {
            rows.remove(0);
        }
    } else {
        has_next = rows.len() as u64 > per_page;
        has_prev = params.after.is_some();
        if has_next {
            rows.truncate(per_page as usize);
        }
    }

    let next_cursor = if has_next {
        rows.last().map(&cursor_fn)
    } else {
        None
    };
    let prev_cursor = if has_prev {
        rows.first().map(&cursor_fn)
    } else {
        None
    };

    Ok(CursorResult {
        data: rows,
        per_page,
        next_cursor,
        prev_cursor,
        has_next,
        has_prev,
    })
}
