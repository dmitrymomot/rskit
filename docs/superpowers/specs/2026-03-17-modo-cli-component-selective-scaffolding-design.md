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

6. **Upload storage backend** (only if uploads selected) — "Storage backend?"
   - Local filesystem
   - S3 (via OpenDAL/RustFS)

### Dependency Auto-Selection

Applied after prompts, before fragment collection. Auto-selected items display a note to the user.

| Selection | Auto-enables | Reason |
|-----------|-------------|--------|
| `auth` | `sessions` | Auth needs session storage |
| `csrf` | `sessions` + `templates` | CSRF uses session + template context |
| `email` | `jobs` | Email sent via job worker |
| Worker app type | `jobs` | Worker exists to run jobs |

**Validation errors** (cannot auto-resolve — re-prompt):
- `sessions` / `jobs` / `tenant` selected but DB layer is "None"
- Upload with S3 selected but app type is Worker

### Display

Auto-selection notes appear between prompts:

```
? Which feature crates do you need?
  [x] Authentication (modo-auth)
  [x] Email (modo-email)

i Authentication requires sessions — sessions enabled
i Email delivery requires jobs — jobs enabled

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
    upload/
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
let jobs = modo_jobs::new(&config.jobs)
    .service(db.clone())
    .run()
    .await?;

## [section: app-services]
.service(jobs)
```

**Section types per target file:**

| Target file | Sections |
|-------------|----------|
| `Cargo.toml` | `dependencies`, `dev-dependencies`, `modo-features` |
| `main.rs` | `imports`, `setup`, `app-services`, `app-modules` |
| `config.rs` | `fields` |
| `config/*.yaml` | `keys` (flat key-value pairs) |
| `.env` / `.env.example` | `vars` |
| `docker-compose.yaml` | `services` |
| `justfile` | `targets` |

Non-`.fragment` files (`.rs`, `.html`, `.gitkeep`, etc.) are copied as-is.

### Fragment Ordering

The `depends_on` field in `[meta]` determines the order of `setup` blocks in `main.rs`. The engine performs a topological sort:

```
db setup          (depends_on: nothing)
session setup     (depends_on: db)
jobs setup        (depends_on: db)
email setup       (depends_on: jobs)
```

## Scaffold Engine

### Pipeline

```
Collect → Merge → Render → Write → Post-scaffold
```

1. **Collect** — Map `ScaffoldConfig` selections to active fragment directories. Order: `base` → `app-type` → `db` → `features` → `modo-features`.

2. **Merge** — For each target file, collect `.fragment` contributions and merge by section:
   - **Cargo.toml** — TOML-aware merge. Modo feature flags consolidate into one dependency line: `modo = { version = "0.3", features = ["templates", "csrf"] }`. Other deps collected into `[dependencies]`.
   - **main.rs** — Concatenate imports (deduplicated), setup blocks (topologically sorted by `depends_on`), service registrations, module registrations.
   - **config.rs** — Collect struct fields into the `Config` struct.
   - **YAML configs** — Key-value merge (later fragments override same key).
   - **.env / .env.example** — Line concatenation with blank line separators.
   - **docker-compose.yaml** — Service block merge.
   - **justfile** — Target concatenation.

3. **Render** — Run MiniJinja on merged output. Variables: `project_name`, `db_driver`, `s3`. Fragments can use Jinja conditionals internally for small variations.

4. **Write** — Write rendered files to project directory. Skip files that render to empty.

5. **Post-scaffold** — `git init`, print summary and next steps.

### Cargo.toml Merge Detail

The engine builds a structured representation:

```rust
struct CargoMerge {
    modo_version: String,
    modo_features: Vec<String>,
    dependencies: BTreeMap<String, DepSpec>,
    dev_dependencies: BTreeMap<String, DepSpec>,
}
```

If no modo features selected: `modo = "0.3"`. Otherwise: `modo = { version = "0.3", features = [...] }`.

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

### Embedding

Fragments are embedded at compile time via `include_dir!` — same approach as current templates. The `fragments/` directory replaces the `templates/` directory.

## CLI Interface

### Command

```
modo new <NAME>
```

No flags. All configuration through interactive prompts.

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
    },
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
- Cargo.toml merge — feature consolidation, dep deduplication
- main.rs merge — topological sort ordering
- Config merge — field collection, YAML key merging

### Integration Tests

Representative combinations (not exhaustive):

| Test case | App type | DB | Features | modo features |
|-----------|----------|----|----------|---------------|
| Bare minimum | Server | None | — | — |
| API-like | Server | modo-db sqlite | — | — |
| Web-like | Server | modo-db sqlite | all | all |
| Worker | Worker | modo-db postgres | jobs | — |
| Mixed | Both | modo-sqlite | sessions, jobs | templates, csrf |
| Postgres full | Server | modo-db postgres | email, jobs | templates |
| Bare SQLite | Server | modo-sqlite | — | — |

Each integration test:
1. Builds `ScaffoldConfig` programmatically (bypasses prompts)
2. Runs scaffold engine
3. Asserts file presence/absence
4. Asserts `Cargo.toml` deps and features
5. Asserts `main.rs` setup block ordering
6. Asserts no unrendered Jinja placeholders (except email templates)
7. (CI only) Asserts `cargo check` passes on generated project

### Dependency Rule Tests

```rust
#[test] fn auth_enables_sessions() { ... }
#[test] fn email_enables_jobs() { ... }
#[test] fn csrf_enables_sessions_and_templates() { ... }
#[test] fn worker_enables_jobs() { ... }
#[test] fn no_db_with_sessions_is_error() { ... }
```

### Fragment Consistency Tests

- Every fragment section name matches a known section in the base skeleton
- Every `depends_on` references an existing fragment
- No duplicate fragment names

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
- `src/merge.rs` — per-file-type merge logic
- `src/scaffold.rs` — rewritten collect → merge → render → write pipeline
- Updated `src/main.rs` — simplified CLI args

### Unchanged

- `minijinja` — still renders Jinja vars within fragments
- `include_dir` — still embeds fragments at compile time
- `clap` — still parses `modo new <name>`
- Project name validation
- Git init post-scaffold
- Test file (rewritten, same location)

### Backwards Compatibility

None. This is a breaking change. `modo new myapp --template web` stops working. Users run `modo new myapp` and select interactively.
