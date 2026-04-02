# Multi-Column Sort for Filter

**Date:** 2026-04-02
**Scope:** `src/db/filter.rs` (primary), tests

## Problem

The current `Filter` system supports only single-column sorting (`sort=-created_at`). Real applications need multi-column ordering (e.g., `sort=priority&sort=-end_date` → `ORDER BY "priority" ASC, "end_date" DESC`). The sort must work with standard HTML form elements (`<select multiple name="sort">`), which produce repeated query parameters.

## Design

### Query String Format

Repeated `sort` params, each with an optional `-` prefix for descending:

```
?sort=priority&sort=-end_date&sort=name
```

Produces: `ORDER BY "priority" ASC, "end_date" DESC, "name" ASC`

### Changes to `Filter` struct

```rust
// Before
pub struct Filter {
    conditions: Vec<FilterCondition>,
    sort: Option<String>,
}

// After
pub struct Filter {
    conditions: Vec<FilterCondition>,
    sort: Vec<String>,
}
```

### Changes to `Filter::from_query_params()`

Instead of `sort = Some(values.first().clone())`, store all values:

```rust
if key == "sort" {
    sort = values.clone();
    continue;
}
```

### Changes to `Filter::validate()` — sort section

Replace the single-field sort validation with a loop that:

1. Iterates over `self.sort` in order
2. Parses `-` prefix for direction
3. Checks against `schema.is_sort_field(field)`
4. Tracks seen fields in a `HashSet<&str>` — first occurrence wins, duplicates skipped
5. Collects validated fragments into a `Vec<String>`
6. Joins with `, ` into `sort_clause: Option<String>` (or `None` if empty)

```rust
let sort_clause = {
    let mut seen = HashSet::new();
    let mut parts = Vec::new();
    for s in &self.sort {
        let (field, desc) = if let Some(stripped) = s.strip_prefix('-') {
            (stripped, true)
        } else {
            (s.as_str(), false)
        };
        if schema.is_sort_field(field) && seen.insert(field) {
            let direction = if desc { "DESC" } else { "ASC" };
            parts.push(format!("\"{field}\" {direction}"));
        }
    }
    if parts.is_empty() { None } else { Some(parts.join(", ")) }
};
```

### No changes to `ValidatedFilter`

`sort_clause` remains `Option<String>`. Multiple columns are joined into one string. This means **zero changes** to `SelectBuilder`, `resolve_order()`, or any downstream consumer.

### No changes to `SelectBuilder`

The existing precedence is preserved:

1. Filter `sort_clause` (from user query string) — if present
2. `SelectBuilder::order_by()` — fallback default if filter has no sort

### Doc string update

The `Filter` doc table row changes:

```
// Before
/// | `sort=field` | Sort ascending; `sort=-field` for descending |

// After
/// | `sort=field` | Sort ascending; `sort=-field` for descending; repeat for multi-column |
```

## Behavior Rules

- **First occurrence wins:** `sort=name&sort=-name` → `ORDER BY "name" ASC`
- **Unknown fields silently dropped:** `sort=unknown&sort=name` → `ORDER BY "name" ASC`
- **All unknown → no sort:** falls back to `SelectBuilder::order_by()` if set
- **Single sort field still works:** fully backward compatible
- **Empty sort → `None`:** same as today

## Test Plan

### Unit tests (`src/db/filter.rs`)

1. **`filter_sort_multi_column`** — `sort=priority&sort=-end_date` produces `"priority" ASC, "end_date" DESC`
2. **`filter_sort_duplicate_first_wins`** — `sort=name&sort=-name` produces `"name" ASC`
3. **`filter_sort_unknown_fields_dropped`** — mix of known and unknown fields, only known survive
4. **`filter_sort_all_unknown_produces_none`** — all unknown → `sort_clause` is `None`
5. **`filter_sort_single_field_backward_compat`** — single `sort=-name` still works as before

### Integration tests (`tests/db_test.rs`)

6. **`select_with_multi_column_sort`** — end-to-end: insert rows, filter with multi-column sort, verify row order

## Files Modified

- `src/db/filter.rs` — `Filter` struct, `from_query_params()`, `validate()`, doc strings
- `tests/db_test.rs` — new integration test
