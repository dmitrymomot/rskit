# modo CLI — Scaffold Tool Design

## Overview

A standalone CLI binary (`modo`) that scaffolds new modo framework applications from predefined templates. Installed via `cargo install modo-cli`, invoked as `modo new <name> --template <template>`.

## CLI Interface

```
modo new <name> --template <template> [--postgres | --sqlite]
modo --help
modo --version
```

### Arguments

| Argument         | Required | Description                                                        |
| ---------------- | -------- | ------------------------------------------------------------------ |
| `<name>`         | yes      | Project directory and crate name                                   |
| `--template, -t` | no       | Template preset: `minimal`, `api`, `web`, `worker`. Default: `web` |
| `--postgres`     | no       | Use Postgres DB driver                                             |
| `--sqlite`       | no       | Use SQLite DB driver (default)                                     |

### Error Behavior

- Missing `<name>` — error with usage message
- Invalid template name — error listing valid options with brief descriptions
- Both `--postgres` and `--sqlite` — error: "conflicting flags"
- `--postgres` or `--sqlite` with `--template minimal` — error: "minimal template does not use a database"
- Target directory already exists — error: "directory already exists"

## Templates

### Template Descriptions

| Template  | Description                                                                                                  |
| --------- | ------------------------------------------------------------------------------------------------------------ |
| `minimal` | Bare modo app with a single handler. Config, env, justfile included. No database.                            |
| `api`     | REST API with database, entities, and JSON handlers.                                                         |
| `web`     | Full web app: db, auth, sessions, templates, jobs, uploads, email, i18n, tenants — everything. **(default)** |
| `worker`  | Background job processor with a health-check HTTP endpoint (no app routes).                                  |

### Generated Structure Per Template

| Directory/File             | minimal | api | web | worker |
| -------------------------- | ------- | --- | --- | ------ |
| `src/main.rs`              | yes     | yes | yes | yes    |
| `src/config.rs`            | yes     | yes | yes | yes    |
| `src/handlers/`            | -       | yes | yes | -      |
| `src/handlers/mod.rs`      | -       | yes | yes | -      |
| `src/models/`              | -       | yes | yes | -      |
| `src/models/mod.rs`        | -       | yes | yes | -      |
| `src/tasks/`               | -       | -   | yes | yes    |
| `src/tasks/mod.rs`         | -       | -   | yes | yes    |
| `src/types.rs`             | -       | yes | yes | -      |
| `src/views/`               | -       | -   | yes | -      |
| `src/views/mod.rs`         | -       | -   | yes | -      |
| `config/development.yaml`  | yes     | yes | yes | yes    |
| `config/production.yaml`   | yes     | yes | yes | yes    |
| `assets/src/app.css`       | -       | -   | yes | -      |
| `assets/static/css/`       | -       | -   | yes | -      |
| `assets/static/js/`        | -       | -   | yes | -      |
| `assets/static/img/`       | -       | -   | yes | -      |
| `templates/app/`           | -       | -   | yes | -      |
| `templates/app/base.html`  | -       | -   | yes | -      |
| `templates/app/index.html` | -       | -   | yes | -      |
| `templates/emails/`        | -       | -   | yes | -      |
| `locales/en/`              | -       | -   | yes | -      |
| `docker-compose.yaml`      | -       | pg  | pg  | pg     |
| `justfile`                 | yes     | yes | yes | yes    |
| `.env`                     | yes     | yes | yes | yes    |
| `.env.example`             | yes     | yes | yes | yes    |
| `.gitignore`               | yes     | yes | yes | yes    |
| `CLAUDE.md`                | yes     | yes | yes | yes    |
| `Cargo.toml`               | yes     | yes | yes | yes    |

### Config Split

- `config/development.yaml` / `config/production.yaml` — server settings, feature flags, non-secret configuration
- `.env` — secrets, API keys, DB connection strings (gitignored via `.gitignore`)

### Config Overrides (web template)

The web template's generated `config/development.yaml` and `config/production.yaml` must include:

- `email.templates_path: "templates/emails"` — overrides modo-email default of `"emails"` to match the `templates/emails/` directory structure
- `i18n.path: "locales"` — matches the `locales/{lang}/` directory structure

### Storage Configuration (web template)

- **Development** (`config/development.yaml`): local filesystem storage (no external dependencies)
- **Production** (`config/production.yaml`): S3-compatible storage (configured via env vars in `.env`)

### Docker Compose (postgres variant only)

When `--postgres` is used with `api`, `web`, or `worker` templates, a `docker-compose.yaml` is generated with:

- **postgres** service: `postgres:18-alpine`, port `5432`, volume for data persistence
- Justfile gains `docker-up` / `docker-down` recipes
- `.env` includes `DATABASE_URL` pointing to the containerized Postgres
- `dev` recipe chains: `docker-up` → `cargo watch -x run`

### Template Feature Matrix

| Feature              | minimal | api | web | worker |
| -------------------- | ------- | --- | --- | ------ |
| modo core            | yes     | yes | yes | yes    |
| modo-db              | -       | yes | yes | yes    |
| modo-auth            | -       | -   | yes | -      |
| modo-session         | -       | -   | yes | -      |
| modo-jobs            | -       | -   | yes | yes    |
| modo-email           | -       | -   | yes | -      |
| modo-upload          | -       | -   | yes | -      |
| modo-tenant          | -       | -   | yes | -      |
| templates feature    | -       | -   | yes | -      |
| csrf feature         | -       | -   | yes | -      |
| i18n feature         | -       | -   | yes | -      |
| sse feature          | -       | -   | yes | -      |
| static-embed feature | -       | -   | yes | -      |

### Worker Template Architecture

The worker template uses `#[modo::main]` and `app.run()` like other templates. It includes:

- A single `/health` endpoint for liveness/readiness checks (useful in k8s/docker)
- Database connection (required by `modo-jobs`)
- Job runner started via `modo_jobs::start()`
- No application routes beyond the health check

This approach requires no framework changes and provides a production-ready health check endpoint out of the box.

### Justfile Recipes Per Template

| Recipe              | minimal | api | web | worker | Description                             |
| ------------------- | ------- | --- | --- | ------ | --------------------------------------- |
| `dev`               | yes     | yes | yes | yes    | `cargo watch -x run` for auto-reload    |
| `build`             | yes     | yes | yes | yes    | Release build                           |
| `fmt`               | yes     | yes | yes | yes    | `cargo fmt`                             |
| `lint`              | yes     | yes | yes | yes    | `cargo clippy -D warnings`              |
| `test`              | yes     | yes | yes | yes    | `cargo test`                            |
| `check`             | yes     | yes | yes | yes    | fmt-check + lint + test                 |
| `css`               | -       | -   | yes | -      | Build Tailwind CSS via standalone CLI   |
| `assets-download`   | -       | -   | yes | -      | Download HTMX, Alpine.js, etc. via curl |
| `tailwind-download` | -       | -   | yes | -      | Download Tailwind CSS standalone CLI    |
| `docker-up`         | -       | pg  | pg  | pg     | Start Postgres container                |
| `docker-down`       | -       | pg  | pg  | pg     | Stop Postgres container                 |

### Frontend Assets (web template only)

**Vendored locally via `just assets-download`:**

| Library            | Destination                      |
| ------------------ | -------------------------------- |
| HTMX               | `assets/static/js/htmx.min.js`   |
| HTMX SSE extension | `assets/static/js/htmx-sse.js`   |
| Alpine.js          | `assets/static/js/alpine.min.js` |

**Tailwind CSS:**

- Standalone CLI binary (no Node.js) downloaded via `just tailwind-download`
- Source: `assets/src/app.css` with Tailwind v4 `@import "tailwindcss"` and `@theme` block
- Output: `assets/static/css/app.css` (compiled, committed to git)
- Build: `just css` runs the Tailwind CLI

**CSS source template (`assets/src/app.css`):**

```css
@import "tailwindcss" source(none);
@source "../../templates";

@theme {
    --font-sans: "Inter", system-ui, sans-serif;
}

.htmx-swapping {
    @apply opacity-0 transition-opacity duration-150 ease-out;
}
.htmx-settling {
    @apply opacity-100 transition-opacity duration-150 ease-in;
}
```

## Implementation

### Crate Structure

New workspace member: `modo-cli/` (must be added to root `Cargo.toml` workspace members).

```
modo-cli/
├── Cargo.toml            # [[bin]] name = "modo"
├── src/
│   ├── main.rs           # Entry point, clap argument parsing, error display
│   ├── scaffold.rs       # Template rendering + file writing logic
│   └── templates.rs      # Embed template directory, load by template name
└── templates/
    ├── shared/           # .gitignore, CLAUDE.md (common to all templates)
    ├── minimal/
    ├── api/
    ├── web/
    └── worker/
```

Each template has its own complete set of files (Cargo.toml, main.rs, config.rs, etc.). The `shared/` directory contains only files that are truly identical across all templates (`.gitignore`, `CLAUDE.md`). No shared Cargo.toml — each template owns its own to avoid complex conditionals.

### Dependencies

| Crate         | Purpose                                         |
| ------------- | ----------------------------------------------- |
| `clap`        | CLI argument parsing with derive                |
| `minijinja`   | Template rendering (already in modo's dep tree) |
| `include_dir` | Embed template directories at compile time      |
| `anyhow`      | Error handling                                  |

### Template Engine

- Template files use `.jinja` extension (stripped when writing output)
- MiniJinja renders templates with context variables
- Conditional sections handled via `{% if %}` blocks (e.g., postgres vs sqlite in Cargo.toml)

### Template Context Variables

| Variable       | Type   | Description                          |
| -------------- | ------ | ------------------------------------ |
| `project_name` | string | Project/crate name from CLI argument |
| `db_driver`    | string | `"sqlite"` or `"postgres"`           |

### Scaffold Process

1. Parse CLI arguments via clap
2. Validate: target directory doesn't exist, flags don't conflict, DB flags not used with `minimal`
3. Load embedded template files for selected template + shared files
4. Build MiniJinja context from CLI arguments
5. For each template file:
   a. Render through MiniJinja
   b. Strip `.jinja` extension
   c. Write to target directory (creating parent dirs as needed)
6. Initialize git repository (`git init`)
7. Print success message with next steps

### Output on Success

```
Created modo project 'myapp' with template 'web' (sqlite)

Next steps:
  cd myapp
  just tailwind-download   # download Tailwind CSS CLI
  just assets-download     # download HTMX, Alpine.js
  just css                 # build CSS
  just dev                 # start dev server
```

For non-web templates:

```
Created modo project 'myapp' with template 'api' (sqlite)

Next steps:
  cd myapp
  just dev
```

## Testing Strategy

- Unit tests: template rendering with known context produces expected output
- Integration tests: run `modo new testapp --template <each>` in a temp dir, verify:
    - All expected files exist
    - `Cargo.toml` is valid TOML with correct dependencies for the template
    - Generated `.gitignore` contains expected entries
- Error case tests: missing args, conflicting flags, existing directory, DB flag with minimal
- Note: `cargo check` on generated projects requires modo crates to be published to crates.io. During development, integration tests verify file structure and content correctness only.
