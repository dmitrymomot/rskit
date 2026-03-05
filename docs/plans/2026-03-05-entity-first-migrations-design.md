# Entity-First Migrations & Auto-Schema Design

## Context

modo needs a schema management strategy that fits its "full magic, single binary" philosophy. The traditional approach — write SQL migrations, run a CLI, generate entity code — requires tooling, multiple files per change, and manual coordination. Instead, modo adopts an **entity-first** approach: define Rust structs, and the framework creates/updates the database schema automatically.

**Built on:** SeaORM v2's `schema-sync` feature (addition-only incremental schema diff) + `entity-registry` + `inventory` auto-discovery.

---

## 1. Core Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Schema approach | Entity-first (hybrid) | Define structs = define schema. Escape hatches for edge cases |
| `#[modo::entity]` macro | Replaces `DeriveEntityModel` entirely | Single macro does everything: SeaORM derives, relation enum, inventory registration, extended attributes |
| Auto-sync behavior | Always sync on startup (all environments) | Sync is addition-only (never drops tables/columns), safe everywhere. Logs all SQL executed. |
| Framework tables | Visible, queryable, framework-owned | Users can `Entity::find()` on framework tables (e.g., list sessions) but don't define the schema |
| Framework table prefix | `_modo_` | Clear separation: `_modo_sessions`, `_modo_jobs`, etc. No collision with user tables |
| Migration escape hatches | Auto-discovered via `inventory` | `#[modo::migration(version = N)]` for data migrations, column drops, type changes |

---

## 2. `#[modo::entity]` Macro

### What It Generates

The `#[modo::entity]` attribute macro is the single entry point for defining database entities. It generates:

1. SeaORM `DeriveEntityModel` + `Entity` + `Column` + `PrimaryKey` + `ActiveModel`
2. `DeriveRelation` enum from inline relation attributes
3. `Related<T>` impls for each relation
4. `ActiveModelBehavior` impl
5. `inventory::submit!` for `EntityRegistration` (auto-discovery)
6. `IndexCreateStatement` entries for composite indices

### Basic Usage

```rust
#[modo::entity(table = "users")]
pub struct User {
    #[entity(primary_key)]
    pub id: i32,

    #[entity(unique)]
    pub email: String,

    #[entity(indexed)]
    pub username: String,

    #[entity(nullable)]
    pub avatar_url: Option<String>,

    #[entity(column_type = "Text")]
    pub bio: String,

    #[entity(default_value = 0)]
    pub credits: i32,

    #[entity(default_expr = "Expr::current_timestamp()")]
    pub created_at: DateTimeUtc,
}
```

### Relations with Foreign Key Actions

```rust
#[modo::entity(table = "posts")]
pub struct Post {
    #[entity(primary_key)]
    pub id: i32,

    #[entity(belongs_to = "User", on_delete = "Cascade")]
    pub user_id: i32,

    pub title: String,

    #[entity(nullable, belongs_to = "Category", on_delete = "SetNull")]
    pub category_id: Option<i32>,
}
```

Supported `on_delete` / `on_update` values: `Cascade`, `SetNull`, `Restrict`, `NoAction`, `SetDefault`.

### Composite Indices

Declared as struct-level attributes:

```rust
#[modo::entity(table = "posts")]
#[entity(index(columns = ["user_id", "created_at"]))]
#[entity(index(columns = ["slug"], unique))]
pub struct Post {
    #[entity(primary_key)]
    pub id: i32,
    pub user_id: i32,
    pub slug: String,
    pub created_at: DateTimeUtc,
}
```

### Many-to-Many Relations

```rust
#[modo::entity(table = "posts")]
pub struct Post {
    #[entity(primary_key)]
    pub id: i32,
    pub title: String,

    #[entity(has_many, via = "PostTag")]
    pub tags: HasMany<Tag>,
}
```

### Column Attribute Reference

| Attribute | Description |
|-----------|-------------|
| `primary_key` | Marks field as primary key |
| `unique` | Adds UNIQUE constraint |
| `indexed` | Creates a single-column index |
| `nullable` | Allows NULL values |
| `column_type = "..."` | Override column type (e.g., `"Text"`, `"Blob"`) |
| `default_value = ...` | Static default value |
| `default_expr = "..."` | SQL expression default (e.g., `"Expr::current_timestamp()"`) |
| `belongs_to = "Entity"` | Foreign key relation (this table owns the FK) |
| `has_many` | One-to-many relation (other table has FK) |
| `has_one` | One-to-one relation (other table has FK) |
| `on_delete = "..."` | FK delete action: Cascade, SetNull, Restrict, NoAction, SetDefault |
| `on_update = "..."` | FK update action: same options as on_delete |
| `via = "JunctionEntity"` | Many-to-many via junction table |
| `from = "col"` | Override FK source column (defaults to field name) |
| `to = "col"` | Override FK target column (defaults to target's PK) |
| `renamed_from = "old_name"` | Column rename — sync will ALTER RENAME instead of ADD |

### Struct-Level Attribute Reference

| Attribute | Description |
|-----------|-------------|
| `table = "name"` | Database table name |
| `index(columns = [...])` | Composite index |
| `index(columns = [...], unique)` | Unique composite index |

---

## 3. Auto-Discovery & Registration

### EntityRegistration

```rust
pub struct EntityRegistration {
    pub table_name: &'static str,
    pub register_fn: fn(SchemaBuilder) -> SchemaBuilder,
    pub is_framework: bool,  // true for _modo_* tables
}

inventory::collect!(EntityRegistration);
```

The `#[modo::entity]` macro generates an `inventory::submit!` block for each entity, just like `#[modo::handler]` does for routes.

### Framework Entity Registration

Framework entities (sessions, jobs, etc.) register themselves identically:

```rust
// Inside modo crate — not user-facing
#[modo::entity(table = "_modo_sessions")]
pub struct ModoSession {
    #[entity(primary_key)]
    pub id: String,           // ULID
    pub token: String,        // hex, indexed
    #[entity(indexed)]
    pub user_id: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub device_name: Option<String>,
    pub device_type: Option<String>,
    pub fingerprint: Option<String>,
    #[entity(column_type = "Text")]
    pub data: String,         // JSON
    pub created_at: DateTimeUtc,
    pub last_active_at: DateTimeUtc,
    pub expires_at: DateTimeUtc,
}
```

Users can query these directly:

```rust
// List user's active sessions for a "manage devices" page
let sessions = modo::session::Entity::find()
    .filter(modo::session::Column::UserId.eq(user.id))
    .filter(modo::session::Column::ExpiresAt.gt(now))
    .all(&db)
    .await?;
```

---

## 4. Startup Schema Sync

### Lifecycle

During `AppBuilder::run()`, after DB connection and WAL pragmas:

1. Collect all `EntityRegistration` entries from `inventory`
2. Collect all `MigrationRegistration` entries from `inventory`
3. Build `SchemaBuilder` from `Schema::new(DbBackend::Sqlite)`
4. Register framework entities first (sorted by dependency order)
5. Register user entities (sorted by dependency order)
6. Call `schema_builder.sync(&db)` — creates missing tables, adds missing columns, renames columns marked with `renamed_from`
7. Run pending `#[modo::migration]` functions (ordered by version, tracked in `_modo_migrations` table)
8. Log all schema changes at `INFO` level

### What Sync Does (SeaORM v2 `schema-sync`)

- Creates tables that don't exist
- Adds columns that are missing from existing tables
- Renames columns annotated with `renamed_from = "old_name"` (via column comment convention)
- Creates missing indices and foreign keys
- **Never drops** tables, columns, or constraints (addition-only)

### What Sync Does NOT Do

- Drop columns (use `#[modo::migration]` escape hatch)
- Change column types (SQLite limitation — use migration)
- Remove indices (use migration)
- Migrate data (use migration)

---

## 5. Migration Escape Hatches

For operations that entity-first can't express (destructive changes, data transformations), modo provides `#[modo::migration]`:

```rust
#[modo::migration(version = 1, description = "Backfill default roles")]
async fn backfill_roles(db: &DatabaseConnection) -> Result<()> {
    db.execute_unprepared(
        "UPDATE users SET role = 'member' WHERE role IS NULL"
    ).await?;
    Ok(())
}

#[modo::migration(version = 2, description = "Drop legacy column")]
async fn drop_legacy_column(db: &DatabaseConnection) -> Result<()> {
    // SQLite doesn't support DROP COLUMN before 3.35.0
    // For older SQLite, this would need table rebuild
    db.execute_unprepared("ALTER TABLE users DROP COLUMN legacy_field").await?;
    Ok(())
}
```

### MigrationRegistration

```rust
pub struct MigrationRegistration {
    pub version: u64,
    pub description: &'static str,
    pub handler: fn(&DatabaseConnection) -> BoxFuture<'_, Result<()>>,
}

inventory::collect!(MigrationRegistration);
```

### Migration Tracking

Executed migrations are tracked in `_modo_migrations`:

```sql
CREATE TABLE _modo_migrations (
    version INTEGER PRIMARY KEY,
    description TEXT NOT NULL,
    executed_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

- Migrations run in `version` order
- Each migration runs exactly once (idempotent by tracking)
- Migrations run **after** schema sync (so new columns exist before data migrations reference them)
- If a migration fails, the application aborts startup with a clear error

### Version Numbering

- Versions are monotonically increasing integers
- Framework reserves versions 0-999 for internal migrations
- User migrations start at version 1000+ (or any number > 999)
- Duplicate version numbers are a compile-time error (detected at startup via inventory scan)

---

## 6. Framework Internal Entities

### Table Inventory

All framework tables use the `_modo_` prefix:

| Table | Feature | Purpose |
|-------|---------|---------|
| `_modo_sessions` | `sessions` | Session persistence |
| `_modo_jobs` | `jobs` | Background job queue |
| `_modo_migrations` | (always) | Migration version tracking |

Future framework tables (added in later phases):

| Table | Feature | Purpose |
|-------|---------|---------|
| `_modo_rate_limits` | `rate-limiting` | Rate limit counters |
| `_modo_webhook_deliveries` | `webhooks` | Webhook delivery log |

### Visibility

- Framework entities are `pub` types under their module (e.g., `modo::session::Entity`, `modo::jobs::Entity`)
- Users can query them with standard SeaORM API
- Users cannot modify the entity struct definitions — those are owned by the framework
- Schema sync handles framework tables alongside user tables in a single pass

### Feature-Gated Registration

Framework entities only register when their feature is enabled:

```rust
#[cfg(feature = "sessions")]
inventory::submit! {
    EntityRegistration {
        table_name: "_modo_sessions",
        register_fn: |sb| sb.register(session::Entity),
        is_framework: true,
    }
}
```

---

## 7. Dev vs Production Behavior

| Behavior | All Environments |
|----------|-----------------|
| Schema sync on startup | Yes — always runs |
| Addition-only (no drops) | Yes — enforced by SeaORM sync |
| Log schema changes | Yes — `INFO` level |
| Run pending migrations | Yes — ordered by version |
| Abort on migration failure | Yes — prevents serving with broken state |

Since sync is addition-only, there is no separate dev/prod behavior. The same binary behaves identically everywhere. This matches modo's single-binary deployment philosophy.

---

## 8. Complete Startup Sequence

Updated lifecycle from architecture doc section 3:

```
1. #[modo::main] expands, collects all auto-discovered routes/jobs/modules/entities/migrations via inventory
2. User calls .service(), .layer(), etc.
3. .run():
   a. Connect to SQLite, enable WAL mode + pragmas
   b. Schema sync: merge framework + user entities, call sync()
   c. Run pending migrations (version-ordered, tracked in _modo_migrations)
   d. Build Router from inventory
   e. Apply middleware
   f. Start job workers (if jobs feature enabled)
   g. Execute startup hooks
   h. Serve with graceful shutdown
```

---

## 9. Example: Full Entity Definitions

```rust
use modo::prelude::*;

// User with soft deletes + timestamps
#[modo::entity(table = "users")]
#[entity(timestamps)]
#[entity(soft_delete)]
pub struct User {
    #[entity(primary_key)]
    pub id: i32,

    #[entity(unique)]
    pub email: String,

    pub name: String,

    #[entity(default_value = "member")]
    pub role: String,

    // created_at, updated_at auto-added by #[entity(timestamps)]
    // deleted_at auto-added by #[entity(soft_delete)]
}

// Post with FK actions, composite indices, tenant scoping
#[modo::entity(table = "posts")]
#[entity(timestamps)]
#[entity(tenant_scoped)]
#[entity(index(columns = ["user_id", "created_at"]))]
#[entity(index(columns = ["slug"], unique))]
pub struct Post {
    #[entity(primary_key)]
    pub id: i32,

    #[entity(belongs_to = "User", on_delete = "Cascade")]
    pub user_id: i32,

    pub title: String,
    pub slug: String,

    #[entity(column_type = "Text")]
    pub body: String,

    #[entity(nullable, belongs_to = "Category", on_delete = "SetNull")]
    pub category_id: Option<i32>,

    // tenant_id auto-added by #[entity(tenant_scoped)]
    // created_at, updated_at auto-added by #[entity(timestamps)]
}

// Simple entity — no extras
#[modo::entity(table = "categories")]
#[entity(timestamps)]
pub struct Category {
    #[entity(primary_key)]
    pub id: i32,

    #[entity(unique)]
    pub name: String,
}

// Junction table with composite PK
#[modo::entity(table = "post_tags")]
pub struct PostTag {
    #[entity(primary_key, belongs_to = "Post", on_delete = "Cascade")]
    pub post_id: i32,
    #[entity(primary_key, belongs_to = "Tag", on_delete = "Cascade")]
    pub tag_id: i32,
}

#[modo::entity(table = "tags")]
pub struct Tag {
    #[entity(primary_key)]
    pub id: i32,

    #[entity(unique)]
    pub name: String,
}

// Migration escape hatch — runs after schema sync
#[modo::migration(version = 1000, description = "Backfill default user roles")]
async fn backfill_roles(db: &DatabaseConnection) -> Result<()> {
    db.execute_unprepared("UPDATE users SET role = 'member' WHERE role IS NULL").await?;
    Ok(())
}
```

On first startup: creates all tables with columns, indices, foreign keys, and auto-generated columns (timestamps, tenant_id, deleted_at). On subsequent startups: syncs any new fields added to the structs. Pending migrations run after sync.

---

## 10. Composite Primary Keys

Supported. Multiple `#[entity(primary_key)]` fields on a single entity create a composite PK. This is essential for junction tables in many-to-many relations.

```rust
#[modo::entity(table = "post_tags")]
pub struct PostTag {
    #[entity(primary_key, belongs_to = "Post", on_delete = "Cascade")]
    pub post_id: i32,
    #[entity(primary_key, belongs_to = "Tag", on_delete = "Cascade")]
    pub tag_id: i32,
}
```

SeaORM natively supports composite PKs, so the macro passes through multiple `#[sea_orm(primary_key)]` annotations.

---

## 11. Soft Deletes

Struct-level `#[entity(soft_delete)]` attribute that:

1. Auto-adds a `deleted_at: Option<DateTimeUtc>` column (nullable, default NULL)
2. Auto-filters all `Entity::find()` queries to include `WHERE deleted_at IS NULL`
3. Provides `.with_deleted()` escape hatch to bypass the filter
4. `delete()` sets `deleted_at = now()` instead of issuing SQL `DELETE`
5. Provides `.force_delete()` for actual row removal

```rust
#[modo::entity(table = "users")]
#[entity(soft_delete)]
pub struct User {
    #[entity(primary_key)]
    pub id: i32,
    pub email: String,
    // deleted_at: Option<DateTimeUtc> auto-added by soft_delete
}

// Usage:
User::find().all(&db).await?;                  // WHERE deleted_at IS NULL
User::find().with_deleted().all(&db).await?;    // no filter
user.delete(&db).await?;                        // SET deleted_at = now()
user.force_delete(&db).await?;                  // actual DELETE
```

### Implementation Strategy

- The macro adds `deleted_at` to the generated `Model` and `Column` enum
- Generates a custom `Select` wrapper (or overrides `Entity::find()`) that applies the default filter
- `with_deleted()` returns a standard `Select` without the filter
- `ActiveModelBehavior::before_delete()` intercepts delete and converts to update
- `force_delete()` bypasses the behavior override

---

## 12. Auto-Timestamps

Struct-level `#[entity(timestamps)]` attribute that:

1. Auto-adds `created_at: DateTimeUtc` column (default: current timestamp, set on insert)
2. Auto-adds `updated_at: DateTimeUtc` column (default: current timestamp, set on every save)

```rust
#[modo::entity(table = "posts")]
#[entity(timestamps)]
pub struct Post {
    #[entity(primary_key)]
    pub id: i32,
    pub title: String,
    // created_at: DateTimeUtc auto-added
    // updated_at: DateTimeUtc auto-added
}
```

### Behavior

- `created_at` — set to `Utc::now()` on insert, never modified after
- `updated_at` — set to `Utc::now()` on every insert and update
- Both implemented via `ActiveModelBehavior::before_save()`
- If user also defines `created_at` or `updated_at` fields explicitly, the macro errors with a clear message (no silent shadowing)

### Interaction with Soft Deletes

When both `#[entity(timestamps)]` and `#[entity(soft_delete)]` are present, `deleted_at` is a third timestamp column. Soft delete sets `deleted_at` but does NOT update `updated_at` (the record isn't being "updated" from the user's perspective — it's being archived).

---

## 13. Tenant Scoping

Struct-level `#[entity(tenant_scoped)]` attribute that:

1. Auto-adds `tenant_id: String` column (NOT NULL, indexed)
2. Auto-scopes all queries via `TenantScoped` trait (adds `WHERE tenant_id = ?`)
3. Auto-sets `tenant_id` on insert from the request's `TenantId` extractor

```rust
#[modo::entity(table = "posts")]
#[entity(tenant_scoped)]
pub struct Post {
    #[entity(primary_key)]
    pub id: i32,
    pub title: String,
    // tenant_id: String auto-added + indexed
}

// All queries auto-scoped:
Post::find().all(&db).await?;  // WHERE tenant_id = 'tenant_abc'

// Cross-tenant (admin use):
Post::find().unscoped().all(&db).await?;  // no tenant filter
```

### Implementation Strategy

- The macro adds `tenant_id` to the generated `Model` and `Column` enum
- Adds a composite index on `(tenant_id, id)` by default
- Integrates with the `TenantId` extractor from modo's multi-tenancy module
- `Entity::find()` override injects the tenant filter from request context
- `.unscoped()` returns a standard `Select` without the tenant filter
- `ActiveModelBehavior::before_save()` sets `tenant_id` from the current tenant context

### Phase Note

The `#[entity(tenant_scoped)]` attribute is designed now to prevent entity definition changes later. Implementation lands in **Phase 3** alongside the multi-tenancy module. Using it before Phase 3 will produce a compile error: "tenant_scoped requires the `tenancy` feature".

---

## 14. Complete Struct-Level Attribute Reference

| Attribute | Description | Phase |
|-----------|-------------|-------|
| `table = "name"` | Database table name | 1 |
| `index(columns = [...])` | Composite index | 1 |
| `index(columns = [...], unique)` | Unique composite index | 1 |
| `timestamps` | Auto-add created_at + updated_at | 1 |
| `soft_delete` | Auto-add deleted_at + query filtering | 2 |
| `tenant_scoped` | Auto-add tenant_id + query scoping | 3 |

---

## 15. Complete Column-Level Attribute Reference

| Attribute | Description | Phase |
|-----------|-------------|-------|
| `primary_key` | Marks field as primary key (supports composite) | 1 |
| `unique` | UNIQUE constraint | 1 |
| `indexed` | Single-column index | 1 |
| `nullable` | Allows NULL | 1 |
| `column_type = "..."` | Override column type (Text, Blob, etc.) | 1 |
| `default_value = ...` | Static default | 1 |
| `default_expr = "..."` | SQL expression default | 1 |
| `belongs_to = "Entity"` | Foreign key (this table owns FK) | 1 |
| `has_many` | One-to-many (other table has FK) | 1 |
| `has_one` | One-to-one (other table has FK) | 1 |
| `on_delete = "..."` | FK delete action | 1 |
| `on_update = "..."` | FK update action | 1 |
| `via = "JunctionEntity"` | Many-to-many junction | 1 |
| `from = "col"` | Override FK source column | 1 |
| `to = "col"` | Override FK target column | 1 |
| `renamed_from = "old"` | Column rename (sync uses ALTER RENAME) | 1 |
