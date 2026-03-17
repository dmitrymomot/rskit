# modo-cli Component-Selective Scaffolding

**Date:** 2026-03-17
**Status:** Draft

## Problem

modo-cli scaffolds new projects using 4 monolithic templates (`minimal`, `api`, `web`, `worker`) with hardcoded dependencies. Users cannot choose which components to include — the `web` template always brings `modo-session`, `modo-jobs`, `modo-email`, `modo-auth`, `modo-upload`, and `modo-tenant`. With the addition of `modo-sqlite` as an alternative DB layer, users also need to choose between `modo-db` (SeaORM ORM) and `modo-sqlite` (pure sqlx).

## Solution

Replace monolithic templates with an **interactive component selector** backed by **composable template fragments**. Users run `modo new myapp` and walk through prompts to pick their app type, database layer, feature crates, and modo feature flags. Each selection activates a set of fragments that are merged into the final project files.

## Interactive Flow

### Prompts (in order)

1. **App type** — "What are you building?"
   - HTTP server
   - Background worker
   - Both (server + worker)

2. **DB layer** — "Which database layer?"
   - modo-db (SeaORM ORM, supports SQLite & PostgreSQL)
   - modo-sqlite (pure sqlx, read/write split, SQLite only)
   - None

3. **DB driver** (only if modo-db selected) — "Which database driver?"
   - SQLite
   - PostgreSQL

4. **Feature crates** (multi-select) — "Which feature crates do you need?"
   - Sessions (`modo-session`)
   - Background jobs (`modo-jobs`)
   - Email (`modo-email`)
   - Authentication (`modo-auth`)
   - File uploads (`modo-upload`)
   - Multi-tenancy (`modo-tenant`)

5. **modo feature flags** (multi-select) — "Which modo features?"
   - Templates (Jinja HTML rendering)
   - CSRF protection
   - i18n / localization
   - SSE (server-sent events)
   - Static file embedding
   - Sentry integration

   Note: `static-fs` (tower-http filesystem serving) is intentionally excluded — it is rarely used in production (most apps use `static-embed` instead). Users can add it manually post-scaffold.

6. **Upload storage backend** (only if uploads selected) — "Storage backend?"
   - Local filesystem
   - S3 (via OpenDAL/RustFS)

### Dependency Auto-Selection

Applied after prompts, before fragment collection. Auto-selected items display a note to the user.

| Selection | Auto-enables | Reason |
|-----------|-------------|--------|
| `auth` | `sessions` | Auth needs session storage |
| `csrf` | `sessions` + `templates` | CSRF uses session + template context |
| `email` | `jobs` | Best practice: email scaffolded with jobs for async delivery (scaffold-level concern, not a crate dependency) |
| Worker app type | `jobs` | Worker exists to run jobs |
| `sessions` + `jobs` both selected | `modo-session` gets `cleanup-job` feature | Enables automatic session cleanup via the job system |
| `auth` + `templates` both selected | `modo-auth` gets `templates` feature | Enables auth-related template helpers |
| `tenant` + `templates` both selected | `modo-tenant` gets `templates` feature | Enables tenant-related template helpers |

### Crate Compatibility Constraints

`modo-session` and `modo-jobs` have hard dependencies on `modo-db` (SeaORM). They do **not** work with `modo-sqlite` alone.

**Validation errors** (cannot auto-resolve — re-prompt or error):
- `sessions` or `jobs` selected but DB layer is `modo-sqlite` → `"Sessions/jobs require modo-db. Please select modo-db as your database layer, or remove sessions/jobs."`
- `sessions` or `jobs` selected but DB layer is "None" → `"Sessions/jobs require modo-db. Please select modo-db as your database layer."`
- `upload` selected but app type is Worker (no HTTP server) → `"File uploads require an HTTP server. Please select 'HTTP server' or 'Both' as your app type, or remove uploads."`

Note: `modo-tenant` does **not** require a database — it is a pure middleware crate for tenant resolution via headers/subdomains. It works with any DB layer or none.

### Display

Auto-selection notes appear between prompts:

```
? Which feature crates do you need?
  [x] Authentication (modo-auth)
  [x] Email (modo-email)

ℹ Authentication requires sessions — sessions enabled
ℹ Email delivery requires jobs — jobs enabled

? Which modo features?
  ...
```

## Fragment Architecture

### Directory Layout

Each selectable component is a fragment directory containing partial snippets for every file it affects:

```
modo-cli/fragments/
  base/                         # Always included
    Cargo.toml.fragment
    src/main.rs.fragment
    src/config.rs.fragment
    config/development.yaml.fragment
    config/production.yaml.fragment
    .env.fragment
    .env.example.fragment
    .gitignore
    CLAUDE.md.jinja
    justfile.fragment

  app-types/
    server/                     # HTTP server skeleton
      src/main.rs.fragment
      src/handlers/mod.rs
      justfile.fragment
    worker/                     # Background worker skeleton
      src/main.rs.fragment
      src/tasks/mod.rs
      justfile.fragment

  db/
    modo-db-sqlite/
      Cargo.toml.fragment
      src/main.rs.fragment
      src/config.rs.fragment
      config/development.yaml.fragment
      config/production.yaml.fragment
      .env.fragment
      .env.example.fragment
      src/models/mod.rs
      data/.gitkeep
    modo-db-postgres/
      Cargo.toml.fragment
      src/main.rs.fragment
      src/config.rs.fragment
      config/development.yaml.fragment
      config/production.yaml.fragment
      .env.fragment
      .env.example.fragment
      docker-compose.yaml.fragment
      justfile.fragment
      src/models/mod.rs
    modo-sqlite/
      Cargo.toml.fragment
      src/main.rs.fragment
      src/config.rs.fragment
      config/development.yaml.fragment
      config/production.yaml.fragment
      .env.fragment
      .env.example.fragment
      src/migrations/.gitkeep
      data/.gitkeep

  features/
    sessions/
      Cargo.toml.fragment
      src/main.rs.fragment
      src/config.rs.fragment
      config/development.yaml.fragment
      config/production.yaml.fragment
    jobs/
      Cargo.toml.fragment
      src/main.rs.fragment
      src/config.rs.fragment
      config/development.yaml.fragment
      config/production.yaml.fragment
      src/tasks/mod.rs
    email/
      Cargo.toml.fragment
      src/main.rs.fragment
      src/config.rs.fragment
      config/development.yaml.fragment
      .env.example.fragment
      docker-compose.yaml.fragment
      src/tasks/send_email.rs
      templates/emails/layouts/default.html
      templates/emails/welcome.md
    auth/
      Cargo.toml.fragment
      src/main.rs.fragment
    upload-local/
      Cargo.toml.fragment
      src/main.rs.fragment
      src/config.rs.fragment
      config/development.yaml.fragment
      config/production.yaml.fragment
    upload-s3/
      Cargo.toml.fragment
      src/main.rs.fragment
      src/config.rs.fragment
      config/development.yaml.fragment
      config/production.yaml.fragment
      .env.example.fragment
      docker-compose.yaml.fragment
    tenant/
      Cargo.toml.fragment
      src/main.rs.fragment

  modo-features/
    templates/
      Cargo.toml.fragment
      src/views/mod.rs
      src/views/home.rs
      templates/app/base.html
      templates/app/index.html
      assets/src/app.css
      assets/static/css/.gitkeep
      assets/static/js/.gitkeep
      assets/static/img/.gitkeep
      justfile.fragment
    csrf/
      Cargo.toml.fragment
    i18n/
      Cargo.toml.fragment
      locales/en/.gitkeep
    sse/
      Cargo.toml.fragment
    static-embed/
      Cargo.toml.fragment
    sentry/
      Cargo.toml.fragment
      src/config.rs.fragment
```

### Fragment File Format

`.fragment` files use section headers to declare where content goes:

```
## [meta]
name = jobs
depends_on = db

## [section: dependencies]
modo-jobs = "0.3"

## [section: imports]
use modo_jobs;

## [section: setup]
{% if db_driver == "sqlite" %}
let jobs_db = modo_db::connect(&config.jobs_database).await?;
modo_db::sync_and_migrate_group(&jobs_db, "jobs").await?;
let jobs = modo_jobs::new(&config.jobs)
    .service(jobs_db)
    .run()
    .await?;
{% else %}
let jobs = modo_jobs::new(&config.jobs)
    .service(db.clone())
    .run()
    .await?;
{% endif %}

## [section: app-services]
.service(jobs)
```

**Jinja conditionals within fragments:** Fragments can use the standard template variables (`project_name`, `db_driver`, `s3`) for internal branching. The jobs fragment above is the most complex example — SQLite needs a separate `jobs_db` connection while Postgres reuses the main `db`. This is the expected pattern for DB-driver-specific variations within a single fragment.

**Section types per target file:**

| Target file | Sections |
|-------------|----------|
| `Cargo.toml` | `dependencies`, `dev-dependencies`, `modo-features`, `cargo-features` |
| `main.rs` | `imports`, `setup`, `app-services`, `app-modules` |
| `config.rs` | `fields` |
| `config/*.yaml` | `keys` (flat key-value pairs) |
| `.env` / `.env.example` | `vars` |
| `docker-compose.yaml` | `services` |
| `justfile` | `targets` |

Non-`.fragment` files (`.rs`, `.html`, `.gitkeep`, etc.) are copied as-is.

### Fragment Ordering

The `depends_on` field in `[meta]` determines the order of `setup` blocks in `main.rs`. The engine performs a topological sort.

**`depends_on` supports abstract categories.** Categories map to whichever fragment is active:
- `db` → one of `modo-db-sqlite`, `modo-db-postgres`, `modo-sqlite`
- `server` → the `server` app-type fragment
- `worker` → the `worker` app-type fragment

**Full dependency graph:**

```
db setup          (depends_on: nothing)
session setup     (depends_on: db)
jobs setup        (depends_on: db)
auth setup        (depends_on: sessions)
email setup       (depends_on: jobs)
upload setup      (depends_on: nothing)
tenant setup      (depends_on: nothing)
```

## Scaffold Engine

### Pipeline

```
Collect → Merge → Render → Write → Post-scaffold
```

1. **Collect** — Map `ScaffoldConfig` selections to active fragment directories. Order: `base` → `app-type` → `db` → `features` → `modo-features`.

2. **Merge** — For each target file, collect `.fragment` contributions and merge by section:
   - **Cargo.toml** — TOML-aware merge. Modo feature flags consolidate into one dependency line: `modo = { version = "0.3", features = ["templates", "csrf"] }`. Other deps collected into `[dependencies]`. Per-crate feature flags handled via `DepSpec` (see below).
   - **main.rs** — Concatenate imports (deduplicated), setup blocks (topologically sorted by `depends_on`), service registrations, module registrations.
   - **config.rs** — Collect struct fields into the `Config` struct.
   - **YAML configs** — Key-value merge (later fragments override same key).
   - **.env / .env.example** — Line concatenation with blank line separators.
   - **docker-compose.yaml** — Service block merge.
   - **justfile** — Target concatenation.

3. **Render** — Run MiniJinja on merged output. Variables: `project_name`, `db_driver`, `s3`. Fragments can use Jinja conditionals internally for variations (e.g., jobs fragment branching on `db_driver`).

4. **Write** — Write rendered files to project directory. Skip files that render to empty.

5. **Post-scaffold** — `git init`, print summary and next steps.

### Cargo.toml Merge Detail

The engine builds a structured representation:

```rust
/// A single dependency entry with optional features and default-features control.
struct DepSpec {
    version: String,
    features: Vec<String>,
    default_features: Option<bool>,  // None = omit (use crate defaults)
}

struct CargoMerge {
    modo_version: String,
    modo_features: Vec<String>,
    dependencies: BTreeMap<String, DepSpec>,
    dev_dependencies: BTreeMap<String, DepSpec>,
    cargo_features: BTreeMap<String, Vec<String>>,  // [features] section: e.g., sentry = ["modo/sentry"]
    lints: Vec<String>,  // cfg feature names for [lints.rust] unexpected_cfgs
}
```

**Output rules:**
- If no modo features selected: `modo = "0.3"`. Otherwise: `modo = { version = "0.3", features = [...] }`.
- `serde = { version = "1", features = ["derive"] }` is always included (from base fragment).
- `modo-db` with Postgres: `modo-db = { version = "0.3", default-features = false, features = ["postgres"] }` — disables the default `sqlite` feature.
- `modo-db` with SQLite: `modo-db = "0.3"` — uses the default `sqlite` feature.
- `modo-upload` with S3: `modo-upload = { version = "0.3", default-features = false, features = ["opendal"] }` — S3-only, no local backend.
- `modo-upload` with local: `modo-upload = "0.3"` — uses the default `local` feature.
- `modo-session` with jobs also selected: `modo-session = { version = "0.3", features = ["cleanup-job"] }`.
- `modo-auth` with templates modo feature: `modo-auth = { version = "0.3", features = ["templates"] }`.
- `modo-tenant` with templates modo feature: `modo-tenant = { version = "0.3", features = ["templates"] }`.
- `[features]` section: generated from fragments that declare cargo features (e.g., sentry: `sentry = ["modo/sentry"]`).
- `[lints.rust]` section: generates `unexpected_cfgs = { level = "warn", check-cfg = ['cfg(feature, values("..."))'] }` from all cargo feature names declared in `[features]`.

**Cross-cutting feature resolution:** After all fragments are collected, the engine runs a post-merge pass that checks for cross-cutting feature interactions (sessions+jobs → cleanup-job, auth+templates → auth templates feature, tenant+templates → tenant templates feature) and adds the appropriate features to the `DepSpec` entries.

**Fragment `[section: dependencies]` examples:**

```
# modo-db-postgres/Cargo.toml.fragment
## [section: dependencies]
modo-db = { version = "0.3", default-features = false, features = ["postgres"] }
```

```
# upload-s3/Cargo.toml.fragment
## [section: dependencies]
modo-upload = { version = "0.3", features = ["opendal"] }
```

```
# sessions/Cargo.toml.fragment
## [section: dependencies]
modo-session = "0.3"
# Note: cleanup-job feature added by cross-cutting resolution if jobs also selected
```

### main.rs Skeleton

The base fragment provides insertion points:

```rust
## [section: imports]
use modo::prelude::*;

## [section: setup]
// (fragments insert here, topologically sorted)

## [section: app-builder]
let app = modo::app::AppBuilder::new()

## [section: app-services]
// (.service() calls)

## [section: app-modules]
// (.module() calls)

## [section: app-run]
    ;
app.run().await?;
```

### Jobs Fragment — SQLite vs Postgres

The jobs fragment is the most complex due to SQLite's dual-database pattern. It uses Jinja conditionals within the fragment:

**`jobs/src/main.rs.fragment`:**
```
## [meta]
name = jobs
depends_on = db

## [section: imports]
use modo_jobs;

## [section: setup]
{% if db_driver == "sqlite" %}
// Jobs use a separate SQLite database
let jobs_db = modo_db::connect(&config.jobs_database).await?;
modo_db::sync_and_migrate_group(&jobs_db, "jobs").await?;
let jobs = modo_jobs::new(&config.jobs)
    .service(jobs_db)
    .run()
    .await?;
{% else %}
// Postgres: jobs share the main database
let jobs = modo_jobs::new(&config.jobs)
    .service(db.clone())
    .run()
    .await?;
{% endif %}

## [section: app-services]
.service(jobs)
```

**`jobs/src/config.rs.fragment`:**
```
## [section: fields]
pub(crate) jobs: modo_jobs::JobsConfig,
{% if db_driver == "sqlite" %}
pub(crate) jobs_database: modo_db::DatabaseConfig,
{% endif %}
```

### Embedding

Fragments are embedded at compile time via `include_dir!` — same approach as current templates. The `fragments/` directory replaces the `templates/` directory.

## CLI Interface

### Command

```
modo new <NAME>
```

Interactive prompts handle all configuration by default.

### Non-Interactive Mode

For CI/CD and scripting, a `--preset` flag provides pre-configured combinations:

```
modo new myapp --preset server-minimal    # Server + no DB + no features
modo new myapp --preset api-sqlite        # Server + modo-db sqlite
modo new myapp --preset api-postgres      # Server + modo-db postgres
modo new myapp --preset web-sqlite        # Server + modo-db sqlite + all features
modo new myapp --preset web-postgres      # Server + modo-db postgres + all features
modo new myapp --preset worker-sqlite     # Worker + modo-db sqlite + jobs
modo new myapp --preset worker-postgres   # Worker + modo-db postgres + jobs
modo new myapp --preset sqlite-raw        # Server + modo-sqlite + no features
modo new myapp --preset full-sqlite       # Both + modo-db sqlite + all features
modo new myapp --preset full-postgres     # Both + modo-db postgres + all features
```

Presets are shortcuts that populate `ScaffoldConfig` and skip prompts. They are not templates — they go through the same fragment engine.

### Argument Parsing (clap)

```rust
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new modo project
    New {
        /// Project name
        name: String,

        /// Use a preset configuration (skips interactive prompts)
        #[arg(long)]
        preset: Option<Preset>,
    },
}

#[derive(Clone, ValueEnum)]
enum Preset {
    ServerMinimal,
    ApiSqlite,
    ApiPostgres,
    WebSqlite,
    WebPostgres,
    WorkerSqlite,
    WorkerPostgres,
    SqliteRaw,
    FullSqlite,
    FullPostgres,
}
```

### ScaffoldConfig

```rust
struct ScaffoldConfig {
    project_name: String,
    app_type: AppType,
    db_layer: DbLayer,
    features: FeatureSet,
    modo_features: ModoFeatures,
    storage_backend: Option<StorageBackend>,
}

enum AppType { Server, Worker, Both }
enum DbLayer { ModoDB(DbDriver), ModoSqlite, None }
enum DbDriver { Sqlite, Postgres }

struct FeatureSet {
    sessions: bool,
    jobs: bool,
    email: bool,
    auth: bool,
    upload: bool,
    tenant: bool,
}

struct ModoFeatures {
    templates: bool,
    csrf: bool,
    i18n: bool,
    sse: bool,
    static_embed: bool,
    sentry: bool,
}

enum StorageBackend { Local, S3 }
```

### Next Steps Output

Adapts to selections:

```
Created modo project 'myapp'

  Components: modo-db (sqlite), sessions, jobs, email
  Features:   templates, csrf, i18n

Next steps:
  cd myapp
  just assets-download     <- only if templates selected
  just docker-up           <- only if postgres or email or s3
  just dev
```

## Testing Strategy

### Unit Tests

- `apply_dependency_rules()` — every auto-selection rule, combinations, no false triggers
- Fragment parsing — `.fragment` files correctly parsed into sections
- Cargo.toml merge — feature consolidation, dep deduplication, per-crate features, `default-features` handling
- Cross-cutting feature resolution — sessions+jobs→cleanup-job, auth+templates→auth templates feature
- main.rs merge — topological sort ordering, abstract category resolution
- Config merge — field collection, YAML key merging

### Integration Tests

Representative combinations (not exhaustive):

| Test case | App type | DB | Features | modo features |
|-----------|----------|----|----------|---------------|
| Bare minimum | Server | None | — | — |
| API-like | Server | modo-db sqlite | — | — |
| Web-like | Server | modo-db sqlite | all | all |
| Worker | Worker | modo-db postgres | jobs | — |
| Postgres full | Server | modo-db postgres | email, jobs | templates |
| Bare modo-sqlite | Server | modo-sqlite | — | — |
| modo-sqlite + tenant | Server | modo-sqlite | tenant | templates |
| All presets | — | — | — | — |

Note: the previously proposed "Mixed: Both + modo-sqlite + sessions, jobs" test case is now invalid — sessions and jobs require modo-db. The "modo-sqlite + tenant" case validates that tenant works without modo-db.

Each integration test:
1. Builds `ScaffoldConfig` programmatically (bypasses prompts)
2. Runs scaffold engine
3. Asserts file presence/absence
4. Asserts `Cargo.toml` deps, features, and `default-features` flags
5. Asserts `main.rs` setup block ordering
6. Asserts no unrendered Jinja placeholders (except email templates)
7. Asserts `[lints.rust]` section matches selected features
8. (CI only) Asserts `cargo check` passes on generated project

### Dependency Rule Tests

```rust
#[test] fn auth_enables_sessions() { ... }
#[test] fn email_enables_jobs() { ... }
#[test] fn csrf_enables_sessions_and_templates() { ... }
#[test] fn worker_enables_jobs() { ... }
#[test] fn no_db_with_sessions_is_error() { ... }
#[test] fn no_db_with_jobs_is_error() { ... }
#[test] fn modo_sqlite_with_sessions_is_error() { ... }
#[test] fn modo_sqlite_with_jobs_is_error() { ... }
#[test] fn tenant_without_db_is_ok() { ... }
#[test] fn upload_with_worker_only_is_error() { ... }
#[test] fn sessions_plus_jobs_adds_cleanup_job_feature() { ... }
#[test] fn auth_plus_templates_adds_auth_templates_feature() { ... }
```

### Fragment Consistency Tests

- Every fragment section name matches a known section in the base skeleton
- Every `depends_on` references an existing fragment name or abstract category (`db`, `server`, `worker`)
- No duplicate fragment names
- Abstract categories resolve to exactly one active fragment per scaffold

## Migration Path

### Removed

- `modo-cli/templates/` directory (all 4 template directories + shared)
- `TemplateType` enum and matching logic in `templates.rs`
- CLI flags: `--template`, `--postgres`, `--sqlite`, `--s3`
- Current `scaffold.rs` Jinja rendering logic

### Added

- `modo-cli/fragments/` directory with all fragment files
- `inquire` dependency in `modo-cli/Cargo.toml`
- `src/interactive.rs` — prompt flow, `ScaffoldConfig`, dependency auto-selection
- `src/fragments.rs` — fragment parsing, section extraction
- `src/merge.rs` — per-file-type merge logic (TOML, Rust, YAML, env, justfile, docker-compose)
- `src/scaffold.rs` — rewritten collect → merge → render → write pipeline
- Updated `src/main.rs` — simplified CLI args, preset support

### Unchanged

- `minijinja` — still renders Jinja vars within fragments
- `include_dir` — still embeds fragments at compile time
- `clap` — still parses `modo new <name>`
- Project name validation
- Git init post-scaffold
- Test file (rewritten, same location)

### Backwards Compatibility

Breaking change. `modo new myapp --template web` stops working. The `--preset` flag provides equivalent functionality for CI/scripting: `modo new myapp --preset web-sqlite`.
