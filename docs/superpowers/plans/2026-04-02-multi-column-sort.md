# Multi-Column Sort Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `Filter` to support multi-column sorting via repeated `sort` query params, producing comma-joined `ORDER BY` clauses.

**Architecture:** Change `Filter.sort` from `Option<String>` to `Vec<String>`, iterate and deduplicate during validation, join into a single `sort_clause: Option<String>`. No changes to `ValidatedFilter`, `SelectBuilder`, or downstream consumers.

**Tech Stack:** Rust, libsql, axum (extractor), `std::collections::HashSet` for dedup

---

## File Map

- **Modify:** `src/db/filter.rs` — `Filter` struct, `from_query_params()`, `validate()`, doc strings
- **Modify:** `tests/db_test.rs` — new unit tests + integration test

---

### Task 1: Update `Filter` struct and parsing

**Files:**
- Modify: `src/db/filter.rs:97-100` (Filter struct)
- Modify: `src/db/filter.rs:128-186` (from_query_params)

- [ ] **Step 1: Write failing unit test for multi-column sort parsing**

Add this test at the bottom of the `#[cfg(test)]` section in `tests/db_test.rs` (after the `filter_sort_unknown_field_ignored` test, around line 508):

```rust
#[test]
fn filter_sort_multi_column() {
    let schema = FilterSchema::new()
        .field("status", FieldType::Text)
        .sort_fields(&["priority", "end_date", "name"]);

    let mut params = HashMap::new();
    params.insert("sort".into(), vec![
        "priority".into(),
        "-end_date".into(),
        "name".into(),
    ]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(
        validated.sort_clause,
        Some("\"priority\" ASC, \"end_date\" DESC, \"name\" ASC".into())
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features db filter_sort_multi_column -- --exact`

Expected: FAIL — current code only takes the first `sort` value, so the sort_clause will be `Some("\"priority\" ASC")` instead of the multi-column version.

- [ ] **Step 3: Update `Filter` struct — change `sort` field type**

In `src/db/filter.rs`, change the `Filter` struct (lines 97-100):

```rust
// Before:
pub struct Filter {
    conditions: Vec<FilterCondition>,
    sort: Option<String>,
}

// After:
pub struct Filter {
    conditions: Vec<FilterCondition>,
    sort: Vec<String>,
}
```

- [ ] **Step 4: Update `from_query_params` — collect all sort values**

In `src/db/filter.rs`, update the `from_query_params` method. Change the `sort` variable initialization (line 130) and the sort branch (lines 133-137), and the struct construction (lines 182-185):

The `sort` variable initialization changes from:

```rust
let mut sort = None;
```

to:

```rust
let mut sort = Vec::new();
```

The sort branch changes from:

```rust
if key == "sort" {
    if let Some(v) = values.first() {
        sort = Some(v.clone());
    }
    continue;
}
```

to:

```rust
if key == "sort" {
    sort = values.clone();
    continue;
}
```

The struct construction stays the same (field name didn't change).

- [ ] **Step 5: Update `validate` — iterate, deduplicate, join sort fields**

In `src/db/filter.rs`, replace the sort validation block (lines 249-262):

```rust
// Before:
let sort_clause = self.sort.and_then(|s| {
    let (field, desc) = if let Some(stripped) = s.strip_prefix('-') {
        (stripped, true)
    } else {
        (s.as_str(), false)
    };
    if schema.is_sort_field(field) {
        let direction = if desc { "DESC" } else { "ASC" };
        Some(format!("\"{field}\" {direction}"))
    } else {
        None // Unknown sort field — ignore
    }
});

// After:
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
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
};
```

Note: `HashSet` is already imported at the top of the file (`use std::collections::{HashMap, HashSet}`).

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test --features db filter_sort_multi_column -- --exact`

Expected: PASS

- [ ] **Step 7: Run all existing tests to verify backward compatibility**

Run: `cargo test --features db`

Expected: All tests pass. The existing `filter_sort` test (single sort field) must still pass — a single-element `Vec` produces the same `Option<String>` as before. The `filter_sort_unknown_field_ignored` test must also still pass.

- [ ] **Step 8: Commit**

```bash
git add src/db/filter.rs tests/db_test.rs
git commit -m "feat(db): support multi-column sort in Filter

Change Filter.sort from Option<String> to Vec<String> to accept
repeated sort query params. Validation deduplicates (first wins)
and joins into a single ORDER BY clause."
```

---

### Task 2: Add remaining unit tests

**Files:**
- Modify: `tests/db_test.rs` — add tests after `filter_sort_multi_column`

- [ ] **Step 1: Write test for duplicate sort fields (first wins)**

Add after the `filter_sort_multi_column` test:

```rust
#[test]
fn filter_sort_duplicate_first_wins() {
    let schema = FilterSchema::new().sort_fields(&["name"]);

    let mut params = HashMap::new();
    params.insert("sort".into(), vec!["name".into(), "-name".into()]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(validated.sort_clause, Some("\"name\" ASC".into()));
}
```

- [ ] **Step 2: Write test for mixed known/unknown fields**

```rust
#[test]
fn filter_sort_unknown_fields_dropped() {
    let schema = FilterSchema::new().sort_fields(&["name", "created_at"]);

    let mut params = HashMap::new();
    params.insert("sort".into(), vec![
        "unknown".into(),
        "-name".into(),
        "password".into(),
        "created_at".into(),
    ]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(
        validated.sort_clause,
        Some("\"name\" DESC, \"created_at\" ASC".into())
    );
}
```

- [ ] **Step 3: Write test for all-unknown fields producing None**

```rust
#[test]
fn filter_sort_all_unknown_produces_none() {
    let schema = FilterSchema::new().sort_fields(&["name"]);

    let mut params = HashMap::new();
    params.insert("sort".into(), vec!["unknown".into(), "password".into()]);

    let filter = Filter::from_query_params(&params);
    let validated = filter.validate(&schema).unwrap();
    assert_eq!(validated.sort_clause, None);
}
```

- [ ] **Step 4: Run all new unit tests**

Run: `cargo test --features db filter_sort -- --nocapture`

Expected: All filter_sort* tests pass:
- `filter_sort` (existing single-field)
- `filter_sort_unknown_field_ignored` (existing)
- `filter_sort_multi_column`
- `filter_sort_duplicate_first_wins`
- `filter_sort_unknown_fields_dropped`
- `filter_sort_all_unknown_produces_none`

- [ ] **Step 5: Commit**

```bash
git add tests/db_test.rs
git commit -m "test(db): add unit tests for multi-column sort edge cases

Cover duplicate-first-wins, mixed known/unknown fields,
and all-unknown-produces-none scenarios."
```

---

### Task 3: Add integration test

**Files:**
- Modify: `tests/db_test.rs` — add integration test after `select_with_sort`

- [ ] **Step 1: Write integration test for multi-column sort with SelectBuilder**

Add after the existing `select_with_sort` test (around line 668):

```rust
#[tokio::test]
async fn select_with_multi_column_sort() {
    let db = test_db().await;
    let conn = db.conn();

    conn.execute(
        "CREATE TABLE tasks (id TEXT PRIMARY KEY, name TEXT NOT NULL, priority INTEGER NOT NULL, status TEXT NOT NULL)",
        (),
    )
    .await
    .unwrap();

    // Insert rows with varying priority and name to verify multi-column ordering
    for (id, name, priority, status) in [
        ("t1", "Deploy", 2, "active"),
        ("t2", "Review", 1, "active"),
        ("t3", "Build", 2, "active"),
        ("t4", "Audit", 1, "active"),
    ] {
        conn.execute(
            "INSERT INTO tasks (id, name, priority, status) VALUES (?1, ?2, ?3, ?4)",
            libsql::params![id, name, priority, status],
        )
        .await
        .unwrap();
    }

    let schema = FilterSchema::new()
        .field("status", FieldType::Text)
        .sort_fields(&["priority", "name"]);

    // Sort by priority ASC, then name ASC as tiebreaker
    let mut params = HashMap::new();
    params.insert("sort".into(), vec!["priority".into(), "name".into()]);
    params.insert("status".into(), vec!["active".into()]);
    let filter = Filter::from_query_params(&params)
        .validate(&schema)
        .unwrap();

    #[derive(serde::Serialize)]
    struct Task {
        id: String,
        name: String,
        priority: i64,
        status: String,
    }
    impl FromRow for Task {
        fn from_row(row: &libsql::Row) -> Result<Self> {
            Ok(Self {
                id: row.get(0)?,
                name: row.get(1)?,
                priority: row.get(2)?,
                status: row.get(3)?,
            })
        }
    }

    let items: Vec<Task> = conn
        .select("SELECT id, name, priority, status FROM tasks")
        .filter(filter)
        .fetch_all()
        .await
        .unwrap();

    assert_eq!(items.len(), 4);
    // priority ASC: 1, 1, 2, 2 — then name ASC within same priority
    assert_eq!(items[0].name, "Audit");    // priority=1, name=Audit
    assert_eq!(items[1].name, "Review");   // priority=1, name=Review
    assert_eq!(items[2].name, "Build");    // priority=2, name=Build
    assert_eq!(items[3].name, "Deploy");   // priority=2, name=Deploy
}
```

- [ ] **Step 2: Run the integration test**

Run: `cargo test --features db select_with_multi_column_sort -- --exact`

Expected: PASS

- [ ] **Step 3: Run full test suite**

Run: `cargo test --features db`

Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add tests/db_test.rs
git commit -m "test(db): add integration test for multi-column sort

Verifies end-to-end multi-column ORDER BY through SelectBuilder
with a tasks table sorted by priority ASC, name ASC."
```

---

### Task 4: Update doc strings

**Files:**
- Modify: `src/db/filter.rs:82-96` (Filter doc comment)

- [ ] **Step 1: Update the `Filter` struct doc string**

In `src/db/filter.rs`, update the doc comment table row for `sort` (line 96):

```rust
// Before:
/// | `sort=field` | Sort ascending; `sort=-field` for descending |

// After:
/// | `sort=field` | Sort ascending; `sort=-field` for descending; repeat for multi-column |
```

- [ ] **Step 2: Run clippy to ensure doc formatting is valid**

Run: `cargo clippy --features db --tests -- -D warnings`

Expected: No warnings or errors.

- [ ] **Step 3: Commit**

```bash
git add src/db/filter.rs
git commit -m "docs(db): update Filter doc string for multi-column sort"
```
