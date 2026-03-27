# Pagination Module Design — 2026-03-27

Offset-based and cursor-based (ID keyset) pagination for modo. Always-available module, no feature flag.

## Decisions

- **Two builders, not one**: `Paginate` (offset) and `CursorPaginate` (cursor) have fundamentally different query mechanics — no shared builder with runtime mode switching.
- **Builder `.bind()` API**: Matches how the rest of the codebase uses sqlx. Internally collects `SqliteArguments`, clones for offset's two-query pattern.
- **ID-based cursors, not opaque**: Modo uses ULIDs (lexicographically time-sorted) for every table. The item ID *is* the cursor — no base64 encoding, no tiebreaker column. Opaque cursors deferred until the filter module introduces arbitrary sort columns.
- **Dedicated extractors**: `PageRequest` and `CursorRequest` implement `FromRequestParts`, read query params, pull `PaginationConfig` from extensions, clamp values. One less line per handler vs `Query<T>` + manual clamping.
- **Zero-config fallback**: Extractors use hardcoded defaults (20/100) if no `PaginationConfig` is in extensions. Module works out of the box.
- **Newest-first default** for cursor pagination (`ORDER BY id DESC`). `.oldest_first()` flips it.
- **Forward-only cursors**: No backward/prev cursor. Simplifies the API for the common case.

## Module Structure

**Location:** `src/page/`

```
src/page/
  mod.rs         — re-exports
  config.rs      — PaginationConfig
  request.rs     — PageRequest, CursorRequest extractors
  response.rs    — Page<T>, CursorPage<T>
  offset.rs      — Paginate builder
  cursor.rs      — CursorPaginate builder
```

## Config

```yaml
pagination:
  default_per_page: 20
  max_per_page: 100
```

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PaginationConfig {
    pub default_per_page: u32,  // default: 20
    pub max_per_page: u32,      // default: 100
}
```

Added to `modo::config::Config` as `pub pagination: PaginationConfig`.

## Request Extractors

Both implement `FromRequestParts`. They deserialize from query params, look for `PaginationConfig` in request extensions (falling back to hardcoded 20/100), and silently clamp out-of-range values.

### PageRequest

Query: `?page=2&per_page=20`

```rust
PageRequest {
    page: u32,       // default 1, clamped to min 1
    per_page: u32,   // default from config, clamped to [1, max_per_page]
}
```

### CursorRequest

Query: `?after=01JQDKV...&per_page=20`

```rust
CursorRequest {
    after: Option<String>,   // None = first page
    per_page: u32,           // default from config, clamped to [1, max_per_page]
}
```

**Clamping behavior:**
- `per_page=0` or negative → 1
- `per_page > max_per_page` → `max_per_page`
- `page=0` or negative → 1
- No errors for out-of-range values — silent clamping

## Result Types

### Page\<T\>

```rust
#[derive(Serialize)]
pub struct Page<T: Serialize> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
    pub has_next: bool,
    pub has_prev: bool,
}
```

`total_pages = ceil(total / per_page)`. `has_prev = page > 1`. `has_next = page < total_pages`.

### CursorPage\<T\>

```rust
#[derive(Serialize)]
pub struct CursorPage<T: Serialize> {
    pub items: Vec<T>,
    pub next: Option<String>,
    pub has_more: bool,
    pub per_page: u32,
}
```

`next` is the last item's `id` if `has_more` is true, `None` otherwise.

## Query Builders

### Paginate (offset)

```rust
let page = Paginate::new("SELECT * FROM users WHERE status = ?")
    .bind("active")
    .fetch::<User>(&pool, &page_req)  // pool: &impl Reader
    .await?;
```

`fetch()` internally:
1. Clones collected `SqliteArguments`
2. Runs `SELECT COUNT(*) FROM ({base_sql})` with cloned args → `total`
3. Computes `offset = (page - 1) * per_page`
4. Runs `{base_sql} LIMIT ? OFFSET ?` with original args + limit/offset
5. Assembles `Page<T>` with computed metadata

Page beyond `total_pages` returns empty `items` with correct metadata (not an error).

### CursorPaginate (ID keyset)

```rust
let page = CursorPaginate::new("SELECT * FROM events WHERE tenant_id = ?")
    .bind(tenant_id)
    .fetch::<Event>(&pool, &cursor_req)  // pool: &impl Reader
    .await?;
```

`fetch()` internally:
1. If `after` is `Some(id)`: wraps SQL as `SELECT * FROM ({base_sql}) WHERE id < ? ORDER BY id DESC LIMIT ?`, binds cursor ID + `per_page + 1`
2. If `after` is `None`: wraps as `SELECT * FROM ({base_sql}) ORDER BY id DESC LIMIT ?`, binds `per_page + 1`
3. If result count > `per_page`: `has_more = true`, drops extra row
4. `next` = last item's `id` if `has_more`

**Convention:** rows must have a TEXT `id` column. Read via `sqlx::Row::get::<String, _>("id")`.

**Ordering:** newest-first by default. `.oldest_first()` flips to `ASC` and changes `<` to `>` in cursor WHERE.

```rust
CursorPaginate::new("SELECT * FROM events")
    .oldest_first()
    .fetch::<Event>(&pool, &cursor_req)
    .await?;
```

### Error handling

- Invalid cursor → `Error::bad_request("invalid cursor")`
- sqlx errors → `Error::internal("query failed").chain(e)`

## Filter Module Composability

The builders accept an optional pre-built WHERE fragment for future filter module integration:

```rust
// Future usage
let (where_sql, filter_args) = filter_builder.to_sql();

let page = Paginate::new("SELECT * FROM users")
    .where_clause(&where_sql, filter_args)
    .fetch::<User>(&pool, &page_req)
    .await?;
```

Internally concatenates `{base_sql} {where_clause}` before applying LIMIT/OFFSET or cursor WHERE. If `.where_clause()` is not called, no fragment is injected. Where clause args are appended after base SQL args.

## Testing

### Unit tests (in `src/page/`)

- `PageRequest` / `CursorRequest` clamping: defaults, min/max, edge cases (`page=0`, `per_page=0`, `per_page=u32::MAX`)
- `Page<T>` metadata: `total_pages`, `has_next`, `has_prev` for various inputs
- `CursorPage<T>` serialization: JSON shape

### Integration tests (`tests/page_test.rs`)

- Offset: correct items, count, and metadata across pages
- Cursor: first page, next page via `after`, final page with `has_more=false`
- Empty result set for both styles
- `.oldest_first()` order verification
- Extractor in handler context: `TestApp` with routes, query params, JSON response validation
