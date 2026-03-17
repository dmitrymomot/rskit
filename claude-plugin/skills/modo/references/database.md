# Database Reference

The database layer in modo is powered by SeaORM v2 RC. It provides connection pooling,
compile-time entity registration via `inventory`, schema synchronization (addition-only),
versioned migrations, pagination helpers, and group-scoped sync for multi-database setups.

> **Important:** modo uses SeaORM **v2** (release candidate). Do not use v1.x APIs, crate docs,
> or patterns. Always consult SeaORM v2 documentation.

## Documentation

- modo-db crate: https://docs.rs/modo-db
- modo-db-macros crate: https://docs.rs/modo-db-macros

---

## Features

| Feature | Effect |
|---------|--------|
| `sqlite` *(default)* | Enables SQLite via `sqlx-sqlite`. Configurable PRAGMAs (WAL mode, busy_timeout, synchronous, foreign_keys, cache_size, mmap_size, etc.) applied per-connection. |
| `postgres` | Enables PostgreSQL via `sqlx-postgres`. |

---

## Setup

### Configuration

`DatabaseConfig` is deserialized from YAML (via `modo::config::load()`). The backend is
selected by setting either the `sqlite` or `postgres` sub-config. If neither is set,
defaults to SQLite with `path: "data/main.db"`.

```rust
use modo_db::DatabaseConfig;
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub struct Config {
    #[serde(flatten)]
    pub core: modo::config::AppConfig,
    pub database: DatabaseConfig,
}
```

`DatabaseConfig` fields:

| Field | Type | Default |
|-------|------|---------|
| `sqlite` | `Option<SqliteDbConfig>` | `Some(SqliteDbConfig::default())` |
| `postgres` | `Option<PostgresDbConfig>` | `None` |
| `max_connections` | `u32` | `5` |
| `min_connections` | `u32` | `1` |
| `acquire_timeout_secs` | `u64` | `30` |
| `idle_timeout_secs` | `u64` | `600` |
| `max_lifetime_secs` | `u64` | `1800` |

`SqliteDbConfig` fields:

| Field | Type | Default |
|-------|------|---------|
| `path` | `String` | `"data/main.db"` |
| `pragmas` | `SqliteConfig` | WAL, busy_timeout=5000, synchronous=NORMAL, foreign_keys=true, cache_size=-2000 |

`PostgresDbConfig` fields:

| Field | Type | Default |
|-------|------|---------|
| `url` | `String` | (required) |

Example YAML (SQLite):

```yaml
database:
  sqlite:
    path: "data/main.db"
  max_connections: 5
  min_connections: 1
```

Example YAML (SQLite with custom PRAGMAs):

```yaml
database:
  sqlite:
    path: "data/main.db"
    pragmas:
      journal_mode: WAL
      busy_timeout: 10000
      synchronous: FULL
      foreign_keys: true
      cache_size: -4000
      mmap_size: 268435456
  max_connections: 5
  min_connections: 1
```

For PostgreSQL:

```yaml
database:
  postgres:
    url: "postgres://user:pass@localhost/myapp"
  max_connections: 10
  min_connections: 2
```

### Connecting and registering

```rust
#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = modo_db::connect(&config.database).await?;
    modo_db::sync_and_migrate(&db).await?;
    app.config(config.core).managed_service(db).run().await
}
```

`modo_db::connect` returns a `DbPool`. Pass it to `app.managed_service(db)` -- this registers
the pool in the service registry and hooks it into graceful shutdown (closes connections on
`SIGTERM`/`SIGINT`).

`modo_db::sync_and_migrate` synchronizes the schema for all registered entities and runs all
pending versioned migrations. Call it once at startup, before `app.run()`.

---

## Entity Definition

Use the `#[modo_db::entity(table = "...")]` attribute macro on a plain Rust struct. The macro
preserves the original struct as a domain type (with auto-derived `Clone`, `Debug`, `Serialize`,
`Default`, `From<Model>`) and generates:

- A SeaORM entity submodule (`Model`, `ActiveModel`, `Entity`, `Column`, `Relation`)
- A `Record` trait implementation with CRUD methods directly on the struct
- An `EntityRegistration` submitted to `inventory` for auto-discovery at startup

### Basic entity

```rust
#[modo_db::entity(table = "todos")]
#[entity(timestamps)]
pub struct Todo {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub title: String,
    #[entity(default_value = false)]
    pub completed: bool,
}
```

The macro generates a module named after the struct in snake_case (e.g., `pub mod todo { ... }`).
Inside it: `Model`, `ActiveModel`, `Entity`, `Column`, `Relation`, and `ActiveModelBehavior`.

### Macro arguments

**`#[modo_db::entity(...)]`** (outer attribute -- table-level):

| Argument | Required | Description |
|----------|----------|-------------|
| `table = "<name>"` | Yes | SQL table name |
| `group = "<name>"` | No | Named group for multi-database setups (default: `"default"`) |

**`#[entity(...)]`** (second attribute -- struct-level options):

| Option | Description |
|--------|-------------|
| `timestamps` | Injects `created_at` and `updated_at: DateTime<Utc>`. Set automatically on insert/update. Do not declare these fields manually. |
| `soft_delete` | Injects `deleted_at: Option<DateTime<Utc>>`. Makes `.delete()` a soft-delete, auto-filters standard queries, generates restore/force-delete methods. |
| `framework` | Marks entity as framework-internal (non-user schema). |
| `index(columns = ["col1", "col2"])` | Creates a composite index. Add `unique` to make it a unique index. |

### Field-level options

Applied as `#[entity(...)]` on individual struct fields:

| Option | Description |
|--------|-------------|
| `primary_key` | Marks the field as the primary key |
| `auto_increment = true\|false` | Overrides SeaORM's default auto-increment behavior |
| `auto = "ulid"\|"short_id"` | Generates a ULID or short ID before insert. Only valid on `primary_key` fields. |
| `unique` | Adds a unique constraint |
| `indexed` | Creates a single-column index |
| `nullable` | Accepted but has no effect -- SeaORM infers nullability from `Option<T>` |
| `column_type = "<type>"` | Overrides the inferred SeaORM column type string |
| `default_value = <literal>` | Sets a column default value |
| `default_expr = "<expr>"` | Sets a default SQL expression string |
| `belongs_to = "<Entity>"` | Declares a `BelongsTo` relation to the named entity |
| `to_column = "<Column>"` | Overrides target column for `belongs_to` FK (default: `"Id"`) |
| `on_delete = "<action>"` | FK action on delete: `Cascade`, `SetNull`, `Restrict`, `NoAction`, `SetDefault` |
| `on_update = "<action>"` | FK action on update: same values as `on_delete` |
| `has_many` | Declares a `HasMany` relation (field excluded from model) |
| `has_one` | Declares a `HasOne` relation (field excluded from model) |
| `via = "<JoinEntity>"` | Used with `has_many`/`has_one` for many-to-many through a join entity |
| `target = "<Entity>"` | Overrides inferred target entity name for `has_many`/`has_one` |
| `renamed_from = "<old_name>"` | Records a rename hint as a column comment |

### ID generation

The `auto = "ulid"` and `auto = "short_id"` options on a primary key field cause
`Record::apply_auto_fields` to call `modo_db::generate_ulid()` or
`modo_db::generate_short_id()` before insert if the field is empty or not set. The `Default`
impl for the struct also calls the generator, so `Todo::default()` produces a struct with a
fresh ULID/short ID.

- `generate_ulid()` -- 26-character Crockford Base32 ULID
- `generate_short_id()` -- 13-character Base36 `[0-9a-z]`, time-sortable

Session IDs and most entity IDs in modo use ULID. Do not use UUID.

---

## Record Trait

The `#[modo_db::entity]` macro implements the `Record` trait on your domain struct. This trait
is the foundation for all generated CRUD and query methods. You never implement `Record` by
hand -- the macro does it.

### What the macro generates on your struct

Given this entity:

```rust
#[modo_db::entity(table = "todos")]
#[entity(timestamps)]
pub struct Todo {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub title: String,
    #[entity(default_value = false)]
    pub completed: bool,
}
```

The macro generates these methods directly on `Todo`:

```rust
impl Todo {
    // --- Find ---
    pub async fn find_by_id(id: &str, db: &impl ConnectionTrait) -> Result<Self, modo::Error>;
    pub async fn find_all(db: &impl ConnectionTrait) -> Result<Vec<Self>, modo::Error>;

    // --- Query builder ---
    pub fn query() -> EntityQuery<Self, todo::Entity>;

    // --- Insert ---
    pub async fn insert(self, db: &impl ConnectionTrait) -> Result<Self, modo::Error>;

    // --- Update ---
    pub async fn update(&mut self, db: &impl ConnectionTrait) -> Result<(), modo::Error>;

    // --- Delete ---
    pub async fn delete(self, db: &impl ConnectionTrait) -> Result<(), modo::Error>;
    pub async fn delete_by_id(id: &str, db: &impl ConnectionTrait) -> Result<(), modo::Error>;

    // --- Bulk operations ---
    pub fn update_many() -> EntityUpdateMany<todo::Entity>;
    pub fn delete_many() -> EntityDeleteMany<todo::Entity>;
}
```

Key differences from raw SeaORM:
- Methods live on the domain struct, not the entity module
- `find_by_id` returns `Result<Todo, Error>` (404 on not found), not `Option<Model>`
- `insert` returns the domain struct with auto-generated fields populated
- `update` mutates `&mut self` in place -- no need to convert to/from `ActiveModel`
- All methods handle error conversion automatically via `db_err_to_error()`

---

## CRUD Patterns

### Create

```rust
use modo::extractor::JsonReq;
use modo::{Json, JsonResult};
use modo_db::Db;

#[modo::handler(POST, "/todos")]
async fn create_todo(
    Db(db): Db,
    input: JsonReq<CreateTodo>,
) -> JsonResult<TodoResponse> {
    input.validate()?;
    let todo = Todo {
        title: input.title.clone(),
        ..Default::default()  // auto-generates ULID, sets timestamps
    }
    .insert(&*db)
    .await?;
    Ok(Json(TodoResponse::from(todo)))
}
```

- Use `..Default::default()` to fill auto-fields (ULID, timestamps).
- `.insert(&*db)` returns the inserted domain struct with all fields populated.
- No `ActiveModel`, `Set()`, or `sea_orm` imports needed.

### Read (single)

```rust
#[modo::handler(GET, "/todos/{id}")]
async fn get_todo(Db(db): Db, id: String) -> JsonResult<TodoResponse> {
    let todo = Todo::find_by_id(&id, &*db).await?;
    Ok(Json(TodoResponse::from(todo)))
}
```

`find_by_id` returns `Result<Todo, modo::Error>`. If the record does not exist, it returns a
404 Not Found error automatically -- no need for `.ok_or(HttpError::NotFound)?`.

### Read (list)

```rust
#[modo::handler(GET, "/todos")]
async fn list_todos(Db(db): Db) -> JsonResult<Vec<TodoResponse>> {
    let todos = Todo::find_all(&*db).await?;
    Ok(Json(todos.into_iter().map(TodoResponse::from).collect()))
}
```

### Update

```rust
#[modo::handler(PATCH, "/todos/{id}")]
async fn toggle_todo(Db(db): Db, id: String) -> JsonResult<TodoResponse> {
    let mut todo = Todo::find_by_id(&id, &*db).await?;
    todo.completed = !todo.completed;
    todo.update(&*db).await?;
    Ok(Json(TodoResponse::from(todo)))
}
```

- Mutate the domain struct fields directly, then call `.update()`.
- `update` refreshes all fields from the database after the write (picks up `updated_at`).
- No `ActiveModel` conversion needed.

### Delete

```rust
#[modo::handler(DELETE, "/todos/{id}")]
async fn delete_todo(Db(db): Db, id: String) -> JsonResult<serde_json::Value> {
    Todo::delete_by_id(&id, &*db).await?;
    Ok(Json(serde_json::json!({"deleted": id})))
}
```

Or load-then-delete when you need the data first:

```rust
let todo = Todo::find_by_id(&id, &*db).await?;
// ... use todo fields ...
todo.delete(&*db).await?;
```

`delete_by_id` loads the record first (triggering `before_delete` hooks), then deletes. For
soft-delete entities, it sets `deleted_at` instead of hard-deleting.

---

## Query Builder

`Todo::query()` returns an `EntityQuery<Todo, todo::Entity>` -- a chainable builder that converts
results to domain types automatically.

### Chainable methods

```rust
use modo_db::sea_orm::ColumnTrait;

// Filter
let incomplete = Todo::query()
    .filter(todo::Column::Completed.eq(false))
    .all(&*db)
    .await?;

// Order
let newest = Todo::query()
    .order_by_desc(todo::Column::CreatedAt)
    .all(&*db)
    .await?;

// Filter + order + limit
let top5 = Todo::query()
    .filter(todo::Column::Completed.eq(false))
    .order_by_desc(todo::Column::CreatedAt)
    .limit(5)
    .all(&*db)
    .await?;

// Offset (for manual pagination -- prefer .paginate() or .paginate_cursor() instead)
let page2 = Todo::query()
    .order_by_asc(todo::Column::CreatedAt)
    .offset(20)
    .limit(10)
    .all(&*db)
    .await?;
```

### Terminal methods

```rust
// Get all matching records
let todos: Vec<Todo> = Todo::query().filter(...).all(&*db).await?;

// Get first matching record (or None)
let maybe: Option<Todo> = Todo::query().filter(...).one(&*db).await?;

// Count matching records
let n: u64 = Todo::query().filter(...).count(&*db).await?;
```

### Pagination (inline)

```rust
use modo_db::PageParams;

let page: PageResult<Todo> = Todo::query()
    .filter(todo::Column::Completed.eq(false))
    .order_by_desc(todo::Column::CreatedAt)
    .paginate(&*db, &PageParams { page: 1, per_page: 20 })
    .await?;
```

```rust
use modo_db::CursorParams;

let page: CursorResult<Todo> = Todo::query()
    .order_by_asc(todo::Column::Id)
    .paginate_cursor(
        todo::Column::Id,
        |m| m.id.clone(),
        &*db,
        &CursorParams::default(),
    )
    .await?;
```

### Joined Queries

`EntityQuery` supports loading related records in a single query via `find_also_related` and
`find_with_related`, which return `JoinedQuery` and `JoinedManyQuery` respectively.

**One-to-one / belongs-to join (`find_also_related`):**

```rust
// Load each todo with its optional author
let results: Vec<(Todo, Option<User>)> = Todo::query()
    .filter(todo::Column::Completed.eq(false))
    .find_also_related::<User, user::Entity>()
    .all(&*db)
    .await?;
```

`JoinedQuery` supports the same chainable methods: `.filter()`, `.order_by_asc()`,
`.order_by_desc()`, `.limit()`, `.offset()`, `.all()`, `.one()`, and `.into_select()`
(which returns a `SelectTwo<E, F>` for raw SeaORM usage).

**One-to-many join (`find_with_related`):**

```rust
// Load each user with all their posts
let results: Vec<(User, Vec<Post>)> = User::query()
    .find_with_related::<Post, post::Entity>()
    .all(&*db)
    .await?;
```

`JoinedManyQuery` supports `.filter()`, `.order_by_asc()`, `.order_by_desc()`, `.limit()`,
`.offset()`, `.all()`, and `.into_select()` (which returns a `SelectTwoMany<E, F>`).

Both require the source entity to implement `sea_orm::Related<F>` for the target entity,
which is generated automatically by `#[entity(belongs_to)]`, `#[entity(has_many)]`, and
`#[entity(has_one)]` field annotations.

### Escape hatch to raw SeaORM

When the `EntityQuery` builder is not expressive enough, unwrap to a raw SeaORM `Select`:

```rust
let select = Todo::query()
    .filter(todo::Column::Completed.eq(false))
    .into_select();  // -> Select<todo::Entity>

// Now use any SeaORM v2 API directly
let results = select
    .inner_join(user::Entity)
    .filter(user::Column::Active.eq(true))
    .all(&*db)
    .await
    .map_err(modo_db::db_err_to_error)?;
```

You can also use the entity module directly for any raw SeaORM operation:

```rust
use modo_db::sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};

let models = todo::Entity::find()
    .filter(todo::Column::Title.contains("urgent"))
    .order_by_asc(todo::Column::CreatedAt)
    .all(&*db)
    .await
    .map_err(modo_db::db_err_to_error)?;

let todos: Vec<Todo> = models.into_iter().map(Todo::from).collect();
```

### Common traits for raw SeaORM queries

| Trait | Purpose |
|-------|---------|
| `EntityTrait` | `find()`, `find_by_id()`, `insert()`, `delete_many()` |
| `ActiveModelTrait` | `.insert()`, `.update()`, `.save()`, `.delete()` on `ActiveModel` |
| `ModelTrait` | `.delete()`, `.into_active_model()` on `Model` |
| `QueryFilter` | `.filter(...)` |
| `QueryOrder` | `.order_by(...)` |
| `ColumnTrait` | `.eq()`, `.ne()`, `.lt()`, `.gt()`, `.is_null()`, `.contains()`, etc. |

Import these from `modo_db::sea_orm`.

---

## Bulk Operations

### Bulk update

`EntityUpdateMany` wraps SeaORM's `UpdateMany` and returns the number of affected rows:

```rust
use modo_db::sea_orm::{ColumnTrait, Expr};

// Mark all overdue todos as completed
let affected = Todo::update_many()
    .filter(todo::Column::Completed.eq(false))
    .filter(todo::Column::CreatedAt.lt(cutoff_date))
    .col_expr(todo::Column::Completed, Expr::value(true))
    .exec(&*db)
    .await?;

tracing::info!("Updated {affected} todos");
```

Use `.col_expr(column, Expr::value(val))` -- there is no `.set()` method on `UpdateMany`.

### Bulk delete

`EntityDeleteMany` wraps SeaORM's `DeleteMany`:

```rust
use modo_db::sea_orm::ColumnTrait;

// Delete all completed todos
let affected = Todo::delete_many()
    .filter(todo::Column::Completed.eq(true))
    .exec(&*db)
    .await?;
```

For soft-delete entities, use `Todo::delete_many()` (soft-deletes: sets `deleted_at`) or
`Todo::force_delete_many()` (hard-deletes: removes rows).

---

## Lifecycle Hooks

Define lifecycle hooks as inherent methods on your entity struct. No trait import is needed --
the `DefaultHooks` blanket trait provides no-op defaults, and your inherent methods take
priority via Rust's method resolution.

### Available hooks

```rust
impl Todo {
    /// Called before insert and update. Can modify self.
    pub fn before_save(&mut self) -> Result<(), modo::Error> {
        self.title = self.title.trim().to_string();
        if self.title.is_empty() {
            return Err(modo::Error::new(
                modo::axum::http::StatusCode::BAD_REQUEST,
                "validation_error",
                "Title cannot be empty",
            ));
        }
        Ok(())
    }

    /// Called after successful insert and update.
    pub fn after_save(&self) -> Result<(), modo::Error> {
        tracing::info!(id = %self.id, "Todo saved");
        Ok(())
    }

    /// Called before delete. Return Err to prevent deletion.
    pub fn before_delete(&self) -> Result<(), modo::Error> {
        if !self.completed {
            return Err(modo::HttpError::Forbidden
                .with_message("Cannot delete incomplete todo"));
        }
        Ok(())
    }
}
```

### Hook execution order

- **Insert:** `before_save` -> DB insert -> `after_save`
- **Update:** `before_save` -> DB update -> refresh fields -> `after_save`
- **Delete:** `before_delete` -> DB delete (or soft-delete)

### Transactional caveat

`after_save` runs after the database write succeeds. If `after_save` returns `Err`, the caller
sees an error, but the row has already been written. For critical post-save validation, use a
transaction (see Transactions section).

---

## Relation Accessors

The entity macro generates async accessor methods for declared relations. These load related
records lazily when called.

### `belongs_to` relation

```rust
#[modo_db::entity(table = "posts")]
#[entity(timestamps)]
pub struct Post {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    #[entity(belongs_to = "User", on_delete = "Cascade")]
    pub user_id: String,
    pub title: String,
    pub body: String,
}
```

The macro generates an async accessor named by stripping the `_id` suffix:

```rust
// Generated:
// pub async fn user(&self, db: &impl ConnectionTrait) -> Result<Option<User>, modo::Error>

let post = Post::find_by_id(&id, &*db).await?;
let author: Option<User> = post.user(&*db).await?;
```

### `has_many` relation

```rust
#[modo_db::entity(table = "users")]
#[entity(timestamps)]
pub struct User {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub email: String,
    #[entity(has_many, target = "Post")]
    pub posts: (),        // field excluded from DB model
}
```

The macro generates an accessor using the field name:

```rust
// Generated:
// pub async fn posts(&self, db: &impl ConnectionTrait) -> Result<Vec<Post>, modo::Error>

let user = User::find_by_id(&id, &*db).await?;
let user_posts: Vec<Post> = user.posts(&*db).await?;
```

### `has_one` relation

```rust
#[modo_db::entity(table = "users")]
pub struct User {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    #[entity(has_one)]
    pub profile: (),
}
```

```rust
// Generated:
// pub async fn profile(&self, db: &impl ConnectionTrait) -> Result<Option<Profile>, modo::Error>

let profile = user.profile(&*db).await?;
```

### Many-to-many via join entity

```rust
#[modo_db::entity(table = "posts")]
pub struct Post {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    #[entity(has_many, via = "PostTag")]
    pub tags: (),
}
```

```rust
let tags: Vec<Tag> = post.tags(&*db).await?;
```

### `target` override

When the target entity cannot be inferred from the field name:

```rust
#[entity(has_many, target = "Comment")]
pub comments: (),
```

---

## Soft Delete

When `#[entity(soft_delete)]` is enabled, the macro transforms delete behavior and adds
recovery methods.

### Entity definition

```rust
#[modo_db::entity(table = "articles")]
#[entity(timestamps, soft_delete)]
pub struct Article {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub title: String,
    pub published: bool,
    // deleted_at: Option<DateTime<Utc>> -- auto-injected, do NOT declare
}
```

### Standard operations auto-filter

```rust
// find_by_id excludes soft-deleted -- returns 404 for deleted records
let article = Article::find_by_id(&id, &*db).await?;

// find_all excludes soft-deleted
let articles = Article::find_all(&*db).await?;

// query() excludes soft-deleted
let published = Article::query()
    .filter(article::Column::Published.eq(true))
    .all(&*db)
    .await?;
```

### Soft-delete a record

```rust
// .delete() sets deleted_at = now (and updates updated_at if timestamps enabled)
let article = Article::find_by_id(&id, &*db).await?;
article.delete(&*db).await?;

// Or by ID
Article::delete_by_id(&id, &*db).await?;
```

### Restore a soft-deleted record

```rust
// Must load with with_deleted() since standard find excludes deleted
let mut article = Article::with_deleted()
    .filter(article::Column::Id.eq(&id))
    .one(&*db)
    .await?
    .ok_or(modo::HttpError::NotFound)?;

article.restore(&*db).await?;  // clears deleted_at
```

### Query including or only soft-deleted

```rust
// All records including soft-deleted
let all = Article::with_deleted().all(&*db).await?;

// Only soft-deleted records
let trashed = Article::only_deleted().all(&*db).await?;
```

### Hard delete (permanent)

```rust
// Force-delete a single record (bypasses soft-delete)
let article = Article::with_deleted()
    .filter(article::Column::Id.eq(&id))
    .one(&*db)
    .await?
    .ok_or(modo::HttpError::NotFound)?;
article.force_delete(&*db).await?;

// Force-delete by ID
Article::force_delete_by_id(&id, &*db).await?;

// Bulk hard-delete
Article::force_delete_many()
    .filter(article::Column::CreatedAt.lt(cutoff))
    .exec(&*db)
    .await?;
```

### Bulk soft-delete

```rust
// Soft-delete all unpublished articles
Article::delete_many()
    .filter(article::Column::Published.eq(false))
    .exec(&*db)
    .await?;
```

`delete_many()` on a soft-delete entity generates an UPDATE (sets `deleted_at = now`) rather
than a DELETE. It auto-filters to only non-deleted records.

---

## Pagination

### Offset pagination

```rust
use modo_db::{Db, PageParams, PageResult};
use modo::extractor::QueryReq;

#[modo::handler(GET, "/todos")]
async fn list_todos(
    Db(db): Db,
    QueryReq(params): QueryReq<PageParams>,
) -> modo::JsonResult<PageResult<TodoResponse>> {
    let page = Todo::query()
        .order_by_desc(todo::Column::CreatedAt)
        .paginate(&*db, &params)
        .await?;
    Ok(modo::Json(page.map(TodoResponse::from)))
}
```

`PageParams` query-string fields (with defaults):

| Field | Default | Constraint |
|-------|---------|------------|
| `page` | `1` | 1-indexed |
| `per_page` | `20` | Clamped to `[1, 100]` |

`PageResult<T>` response fields:

| Field | Type | Description |
|-------|------|-------------|
| `data` | `Vec<T>` | Page items |
| `page` | `u64` | Current page number |
| `per_page` | `u64` | Items per page (after clamping) |
| `has_next` | `bool` | Whether a next page exists |
| `has_prev` | `bool` | Whether a previous page exists |

Uses the limit+1 trick to detect `has_next` without a `COUNT` query.

`PageResult::map<U>(f)` transforms every item in `data` -- use it to convert from domain type
to a response type.

### Cursor pagination

Preferable for large datasets because it avoids offset scans:

```rust
use modo_db::{CursorParams, CursorResult, Db};
use modo::extractor::QueryReq;

#[modo::handler(GET, "/todos")]
async fn list_todos(
    Db(db): Db,
    QueryReq(params): QueryReq<CursorParams>,
) -> modo::JsonResult<CursorResult<TodoResponse>> {
    let page = Todo::query()
        .paginate_cursor(
            todo::Column::Id,
            |m| m.id.clone(),
            &*db,
            &params,
        )
        .await?;
    Ok(modo::Json(page.map(TodoResponse::from)))
}
```

`CursorParams<V = String>` query-string fields:

| Field | Default | Description |
|-------|---------|-------------|
| `per_page` | `20` | Clamped to `[1, 100]` |
| `after` | `None` | Cursor value -- fetch records after this position (forward) |
| `before` | `None` | Cursor value -- fetch records before this position (backward) |

When both `after` and `before` are set, `after` takes precedence.

For string-keyed entities (ULID/NanoID), use `CursorParams` (default `String`). For integer
primary keys, use `CursorParams<i64>`.

`CursorResult<T>` response fields:

| Field | Type | Description |
|-------|------|-------------|
| `data` | `Vec<T>` | Page items |
| `per_page` | `u64` | Items per page (after clamping) |
| `next_cursor` | `Option<String>` | Cursor for `?after=` to get the next page |
| `prev_cursor` | `Option<String>` | Cursor for `?before=` to get the previous page |
| `has_next` | `bool` | Whether a next page exists |
| `has_prev` | `bool` | Whether a previous page exists |

Navigate with `?after=<next_cursor>` (forward) and `?before=<prev_cursor>` (backward).

### Standalone pagination functions

The `paginate` and `paginate_cursor` free functions accept a raw `Select<E>` instead of an
`EntityQuery`. Use these when building queries with raw SeaORM:

```rust
use modo_db::{paginate, paginate_cursor};
use modo_db::sea_orm::EntityTrait;

let page = paginate(todo::Entity::find(), &*db, &params).await
    .map_err(modo_db::db_err_to_error)?;
```

---

## Transactions

Use SeaORM's transaction API directly. The `&*db` deref gives you a `DatabaseConnection` which
supports `.begin()`:

```rust
use modo_db::sea_orm::TransactionTrait;

#[modo::handler(POST, "/transfer")]
async fn transfer(Db(db): Db, input: JsonReq<TransferInput>) -> JsonResult<()> {
    let txn = db.begin().await.map_err(modo_db::db_err_to_error)?;

    let mut from = Account::find_by_id(&input.from_id, &txn).await?;
    let mut to = Account::find_by_id(&input.to_id, &txn).await?;

    from.balance -= input.amount;
    to.balance += input.amount;

    from.update(&txn).await?;
    to.update(&txn).await?;

    txn.commit().await.map_err(modo_db::db_err_to_error)?;
    Ok(Json(()))
}
```

All Record trait methods (`insert`, `update`, `delete`, `find_by_id`, etc.) accept
`&impl ConnectionTrait`, so they work with both `&*db` and `&txn`.

---

## Error Mapping

The `db_err_to_error` helper converts SeaORM errors to `modo::Error` with appropriate HTTP
status codes:

| SeaORM Error | HTTP Status |
|---|---|
| `UniqueConstraintViolation` (via `DbErr::sql_err()`) | 409 Conflict |
| `ForeignKeyConstraintViolation` (via `DbErr::sql_err()`) | 409 Conflict |
| `DbErr::RecordNotFound` | 404 Not Found |
| Anything else | 500 Internal Server Error |

All generated `Record` trait methods call `db_err_to_error` automatically. Use it explicitly
only when calling raw SeaORM APIs:

```rust
let model = todo::Entity::find_by_id(&id)
    .one(&*db)
    .await
    .map_err(modo_db::db_err_to_error)?;
```

---

## Migrations

### Auto-migration (schema sync)

`sync_and_migrate` runs an addition-only schema sync. It creates tables and columns that do not
yet exist; it never drops or renames columns. This is suitable for development and simple
production deployments.

The framework bootstraps a `_modo_migrations` table to track executed versioned migrations.

### Versioned SQL migrations

For data migrations, backfills, index changes, or any operation that schema sync cannot handle,
use `#[modo_db::migration]`:

```rust
#[modo_db::migration(version = 1, description = "seed default roles")]
async fn seed_roles(db: &sea_orm::DatabaseConnection)
    -> Result<(), modo::Error>
{
    // execute_unprepared returns Result<_, DbErr>; use map_err to convert
    db.execute_unprepared(
        "INSERT INTO roles (id, name) VALUES ('admin', 'Administrator')"
    ).await.map_err(|e| modo::Error::internal(e.to_string()))?;
    Ok(())
}
```

**Required arguments:**

| Argument | Type | Description |
|----------|------|-------------|
| `version = <u64>` | integer | Monotonically increasing migration version |
| `description = "<text>"` | string | Human-readable description shown in logs |

**Optional argument:**

| Argument | Description |
|----------|-------------|
| `group = "<name>"` | Assigns migration to a named group (default: `"default"`) |

The annotated function must be `async`, accept a single `&sea_orm::DatabaseConnection` parameter, and
return `Result<(), modo::Error>`. The macro keeps the function as-is and submits a
`MigrationRegistration` to `inventory`.

Migrations are version-ordered and deduplicated at startup. Duplicate version numbers across
registrations are a runtime error (detected before any migration runs).
Each executed migration is recorded in `_modo_migrations` with its version, description, and
timestamp.

### Migration using SeaORM typed API

Prefer the typed SeaORM API over raw SQL when possible:

```rust
use modo_db::sea_orm::{ActiveModelTrait, Set};

#[modo_db::migration(version = 2, description = "normalize emails")]
async fn normalize_emails(db: &sea_orm::DatabaseConnection)
    -> Result<(), modo::Error>
{
    use modo_db::sea_orm::{EntityTrait, ColumnTrait, QueryFilter};

    let users = user::Entity::find().all(db).await
        .map_err(|e| modo::Error::internal(e.to_string()))?;
    for u in users {
        let mut am: user::ActiveModel = u.into();
        if let sea_orm::ActiveValue::Set(ref mut email) = am.email {
            *email = email.to_lowercase();
        }
        am.update(db).await
            .map_err(|e| modo::Error::internal(e.to_string()))?;
    }
    Ok(())
}
```

---

## Db Extractor

`Db` is an axum extractor that retrieves the `DbPool` from the service registry.

```rust
use modo_db::Db;

#[modo::handler(GET, "/todos")]
async fn list_todos(Db(db): Db) -> modo::JsonResult<Vec<TodoResponse>> {
    let todos = Todo::find_all(&*db).await?;
    Ok(modo::Json(todos.into_iter().map(TodoResponse::from).collect()))
}
```

`DbPool` implements `Deref<Target = DatabaseConnection>`, so `&*db` gives a
`&sea_orm::DatabaseConnection` suitable for passing to SeaORM query methods.
`DbPool` also has a `.connection()` method that returns the same `&DatabaseConnection`.

`Db` returns a `500 Internal Server Error` if `DbPool` was not registered via
`app.managed_service(db)`.

---

## Group-Scoped Sync

Entities and migrations can be assigned to a named group. The default group is `"default"`.
`sync_and_migrate` syncs all groups; `sync_and_migrate_group` syncs only the named group.

Use groups when different entity sets live in different databases (e.g., a main app database
and a separate analytics or jobs database):

```rust
// Entity in the "analytics" group
#[modo_db::entity(table = "events", group = "analytics")]
pub struct Event {
    #[entity(primary_key, auto = "ulid")]
    pub id: String,
    pub name: String,
}

// Migration in the "analytics" group
#[modo_db::migration(version = 1, description = "seed event types", group = "analytics")]
async fn seed_event_types(db: &sea_orm::DatabaseConnection)
    -> Result<(), modo::Error>
{
    // ...
    Ok(())
}

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    // Main database -- syncs all registered entities and runs pending migrations
    let db = modo_db::connect(&config.database).await?;
    modo_db::sync_and_migrate(&db).await?;

    // Analytics database -- syncs only "analytics" group
    let analytics_db = modo_db::connect(&config.analytics_database).await?;
    modo_db::sync_and_migrate_group(&analytics_db, "analytics").await?;

    app.config(config.core)
        .managed_service(db)
        .managed_service(analytics_db)
        .run()
        .await
}
```

`sync_and_migrate` with no group filter syncs all registered entities regardless of group
(which may cause issues if entities from different groups reference different databases). For
multi-database setups, always use `sync_and_migrate_group` per database.

---

## Integration Patterns

### Multiple databases

Register each `DbPool` as a managed service under a distinct wrapper type (newtype) so they are
addressable separately from the service registry:

```rust
// Define newtypes so the service registry can distinguish them
pub struct MainDb(pub DbPool);
pub struct AnalyticsDb(pub DbPool);

// In main:
let main_db = modo_db::connect(&config.database).await?;
let analytics_db = modo_db::connect(&config.analytics_database).await?;
modo_db::sync_and_migrate(&main_db).await?;
modo_db::sync_and_migrate_group(&analytics_db, "analytics").await?;

app.managed_service(MainDb(main_db))
   .managed_service(AnalyticsDb(analytics_db))
   .run()
   .await
```

Extract the secondary database using `modo::Service<AnalyticsDb>` in handlers.

### Response mapping

Define a `From<Todo>` implementation on your response type. Because `Todo` is the domain struct
(not the SeaORM `Model`), you work with your own type everywhere:

```rust
#[derive(serde::Serialize)]
pub struct TodoResponse {
    id: String,
    title: String,
    completed: bool,
}

impl From<Todo> for TodoResponse {
    fn from(t: Todo) -> Self {
        Self {
            id: t.id,
            title: t.title,
            completed: t.completed,
        }
    }
}
```

Then use `.map(TodoResponse::from)` on `Vec<Todo>` or on `PageResult::map` / `CursorResult::map`.

### Partial update with raw SeaORM

When you need to update specific fields without loading the full record, use the generated
`ActiveModel` directly:

```rust
use modo_db::sea_orm::{ActiveModelTrait, Set};

// Update a single field by PK
let mut am = todo::ActiveModel {
    id: Set(id.clone()),
    completed: Set(true),
    ..Default::default()  // other fields are NotSet -> not written
};
am.update(&*db).await.map_err(modo_db::db_err_to_error)?;
```

---

## Gotchas

- **SeaORM v2 only**: modo uses SeaORM v2 RC. Do not reference SeaORM v1.x crate docs,
  migration patterns, or API surface.

- **`find_by_id` returns `Result`, not `Option`**: Unlike raw SeaORM, the generated
  `find_by_id` returns `Result<Self, modo::Error>` with a 404 error when the record is missing.
  There is no `.ok_or()` needed.

- **`update` mutates `&mut self`**: After calling `.update(&*db)`, the struct is refreshed with
  the latest values from the database (including updated timestamps). You do not need to
  re-fetch the record.

- **`ExprTrait` conflicts with `Ord::max`/`Ord::min`**: SeaORM's `ExprTrait` re-exports methods
  named `max` and `min`. Disambiguate with the fully-qualified form: `Ord::max(a, b)`.

- **`inventory` linking in tests**: Entity and migration registrations submitted via
  `inventory::submit!` may be dropped by the linker in test builds if nothing from the module is
  directly referenced. Force linking with `use crate::entity::todo as _;` in test files.

- **Schema sync is addition-only**: `sync_and_migrate` never drops or renames columns. Use a
  versioned `#[modo_db::migration]` to rename or drop columns.

- **`auto = "ulid"` only on primary key fields**: Using `auto = "ulid"` or `auto = "short_id"` on
  a non-primary-key field is a compile error.

- **Do not declare `created_at`/`updated_at`/`deleted_at` manually**: When `timestamps` or
  `soft_delete` is enabled, the macro injects these fields. Declaring them yourself is a compile
  error.

- **`Db` extractor requires `managed_service`**: If `DbPool` is not registered via
  `app.managed_service(db)`, extracting `Db` in a handler returns a `500 Internal Server Error`.

- **`&*db` dereference**: `DbPool` implements `Deref<Target = DatabaseConnection>`. Pass `&*db`
  (or `db.connection()`) to SeaORM and Record trait methods.

- **Duplicate migration versions**: Two `#[modo_db::migration(version = N)]` entries with the
  same `N` (in the same group) cause `sync_and_migrate` to return an error before any migrations
  run. Version numbers must be unique per group.

- **Soft-delete relation accessors do not auto-filter**: The generated `belongs_to`/`has_many`/
  `has_one` accessor methods query raw SeaORM entities and do NOT automatically filter by
  `deleted_at IS NULL`. Apply your own filter if needed.

- **`after_save` transactional gap**: The row is written before `after_save` runs. If
  `after_save` returns `Err`, the caller sees an error but the row exists in the database.
  Use an explicit transaction if this matters.

- **Bulk update uses `.col_expr()`, not `.set()`**: SeaORM's `UpdateMany` does not have a
  `.set()` method. Use `.col_expr(column, Expr::value(val))` instead.

- **`db_err_to_error` maps constraint violations to 409**: Unique and FK violations become
  `409 Conflict`. The orphan rule prevents `impl From<DbErr> for modo::Error` in modo-db, so
  use the helper function explicitly when calling raw SeaORM APIs.

- **Raw SQL in migrations requires explicit error conversion**: `execute_unprepared` returns
  `Result<_, sea_orm::DbErr>`. Use `.map_err(|e| modo::Error::internal(e.to_string()))?` --
  the `?` operator alone will not compile because `DbErr` has no `From` conversion to
  `modo::Error`.

---

## Key Type Reference

| Type | Path |
|------|------|
| `DatabaseConfig` | `modo_db::DatabaseConfig` |
| `SqliteDbConfig` | `modo_db::SqliteDbConfig` |
| `PostgresDbConfig` | `modo_db::PostgresDbConfig` |
| `SqliteConfig` | `modo_db::SqliteConfig` |
| `DbPool` | `modo_db::DbPool` |
| `Db` | `modo_db::Db` |
| `Record` | `modo_db::Record` |
| `DefaultHooks` | `modo_db::DefaultHooks` |
| `EntityQuery<T, E>` | `modo_db::EntityQuery` |
| `EntityUpdateMany<E>` | `modo_db::EntityUpdateMany` |
| `EntityDeleteMany<E>` | `modo_db::EntityDeleteMany` |
| `JoinedQuery<T, U, E, F>` | `modo_db::JoinedQuery` |
| `JoinedManyQuery<T, U, E, F>` | `modo_db::JoinedManyQuery` |
| `EntityRegistration` | `modo_db::EntityRegistration` |
| `MigrationRegistration` | `modo_db::MigrationRegistration` |
| `PageParams` | `modo_db::PageParams` |
| `PageResult<T>` | `modo_db::PageResult` |
| `CursorParams<V>` | `modo_db::CursorParams` |
| `CursorResult<T>` | `modo_db::CursorResult` |
| `paginate` | `modo_db::paginate` |
| `paginate_cursor` | `modo_db::paginate_cursor` |
| `sync_and_migrate` | `modo_db::sync_and_migrate` |
| `sync_and_migrate_group` | `modo_db::sync_and_migrate_group` |
| `connect` | `modo_db::connect` |
| `generate_ulid` | `modo_db::generate_ulid` |
| `generate_short_id` | `modo_db::generate_short_id` |
| `db_err_to_error` | `modo_db::db_err_to_error` |
