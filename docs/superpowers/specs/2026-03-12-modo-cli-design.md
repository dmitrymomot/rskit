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

| Argument | Required | Description |
|----------|----------|-------------|
| `<name>` | yes | Project directory and crate name |
| `--template, -t` | yes | Template preset: `minimal`, `api`, `web`, `worker` |
| `--postgres` | no | Use Postgres DB driver |
| `--sqlite` | no | Use SQLite DB driver (default) |

### Error Behavior

- Missing `<name>` — error with usage message
- Missing `--template` — error listing available templates with brief descriptions
- Invalid template name — error listing valid options
- Both `--postgres` and `--sqlite` — error: "conflicting flags"
- Target directory already exists — error: "directory already exists"

## Templates

### Template Descriptions

| Template | Description |
|----------|-------------|
| `minimal` | Bare modo app with a single handler. Config, env, justfile included. |
| `api` | REST API with database, entities, and JSON handlers. |
| `web` | Full web app: db, auth, sessions, templates, jobs, uploads, email, tenants — everything. |
| `worker` | Background job processor only, no HTTP server. |

### Generated Structure Per Template

| Directory/File | minimal | api | web | worker |
|---|---|---|---|---|
| `src/main.rs` | yes | yes | yes | yes |
| `src/config.rs` | yes | yes | yes | yes |
| `src/handlers/` | - | yes | yes | - |
| `src/handlers/mod.rs` | - | yes | yes | - |
| `src/models/` | - | yes | yes | - |
| `src/models/mod.rs` | - | yes | yes | - |
| `src/tasks/` | - | - | yes | yes |
| `src/tasks/mod.rs` | - | - | yes | yes |
| `src/views/` | - | - | yes | - |
| `src/views/mod.rs` | - | - | yes | - |
| `config/development.yml` | yes | yes | yes | yes |
| `config/production.yml` | yes | yes | yes | yes |
| `assets/src/app.css` | - | - | yes | - |
| `assets/static/css/` | - | - | yes | - |
| `assets/static/js/` | - | - | yes | - |
| `assets/static/img/` | - | - | yes | - |
| `templates/app/` | - | - | yes | - |
| `templates/app/base.html` | - | - | yes | - |
| `templates/app/index.html` | - | - | yes | - |
| `templates/emails/` | - | - | yes | - |
| `justfile` | yes | yes | yes | yes |
| `.env` | yes | yes | yes | yes |
| `.env.example` | yes | yes | yes | yes |
| `.gitignore` | yes | yes | yes | yes |
| `CLAUDE.md` | yes | yes | yes | yes |
| `Cargo.toml` | yes | yes | yes | yes |

### Config Split

- `config/development.yml` / `config/production.yml` — server settings, feature flags, non-secret configuration
- `.env` — secrets, API keys, DB connection strings (gitignored via `.gitignore`)

### Template Feature Matrix

| Feature | minimal | api | web | worker |
|---------|---------|-----|-----|--------|
| modo core | yes | yes | yes | yes |
| modo-db | - | yes | yes | yes |
| modo-auth | - | - | yes | - |
| modo-session | - | - | yes | - |
| modo-jobs | - | - | yes | yes |
| modo-email | - | - | yes | - |
| modo-upload | - | - | yes | - |
| modo-tenant | - | - | yes | - |
| templates feature | - | - | yes | - |
| csrf feature | - | - | yes | - |
| sse feature | - | - | yes | - |
| static-embed feature | - | - | yes | - |

### Justfile Recipes Per Template

| Recipe | minimal | api | web | worker | Description |
|--------|---------|-----|-----|--------|-------------|
| `dev` | yes | yes | yes | yes | Run with cargo-watch for hot reload |
| `build` | yes | yes | yes | yes | Release build |
| `fmt` | yes | yes | yes | yes | `cargo fmt` |
| `lint` | yes | yes | yes | yes | `cargo clippy -D warnings` |
| `test` | yes | yes | yes | yes | `cargo test` |
| `check` | yes | yes | yes | yes | fmt-check + lint + test |
| `css` | - | - | yes | - | Build Tailwind CSS via standalone CLI |
| `assets-download` | - | - | yes | - | Download HTMX, Alpine.js, etc. via curl |
| `tailwind-download` | - | - | yes | - | Download Tailwind CSS standalone CLI |

### Frontend Assets (web template only)

**Vendored locally via `just assets-download`:**

| Library | Destination |
|---------|-------------|
| HTMX | `assets/static/js/htmx.min.js` |
| HTMX SSE extension | `assets/static/js/htmx-sse.js` |
| Alpine.js | `assets/static/js/alpine.min.js` |
| Tailwind Elements | `assets/static/js/elements.min.js` |

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

New workspace member: `modo-cli/`

```
modo-cli/
├── Cargo.toml
├── src/
│   ├── main.rs         # Entry point, clap argument parsing, error display
│   ├── scaffold.rs     # Template rendering + file writing logic
│   └── templates.rs    # Embed template directory, load by template name
└── templates/
    ├── shared/         # Files common to all templates (.gitignore, etc.)
    ├── minimal/
    ├── api/
    ├── web/
    └── worker/
```

### Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing with derive |
| `minijinja` | Template rendering (already in modo's dep tree) |
| `include_dir` | Embed template directories at compile time |
| `anyhow` | Error handling |

### Template Engine

- Template files use `.jinja` extension (stripped when writing output)
- MiniJinja renders templates with context variables
- Conditional sections handled via `{% if %}` blocks (e.g., postgres vs sqlite in Cargo.toml)

### Template Context Variables

| Variable | Type | Description |
|----------|------|-------------|
| `project_name` | string | Project/crate name from CLI argument |
| `db_driver` | string | `"sqlite"` or `"postgres"` |

### Scaffold Process

1. Parse CLI arguments via clap
2. Validate: target directory doesn't exist, flags don't conflict
3. Load embedded template files for selected template + shared files
4. Build MiniJinja context from CLI arguments
5. For each template file:
   a. Render through MiniJinja
   b. Strip `.jinja` extension
   c. Write to target directory (creating parent dirs as needed)
6. Print success message with next steps

### Output on Success

```
Created modo project 'myapp' with template 'api' (sqlite)

Next steps:
  cd myapp
  just dev
```

For `web` template, additionally:

```
  just tailwind-download   # download Tailwind CSS CLI
  just assets-download     # download HTMX, Alpine.js
  just css                 # build CSS
```

## Testing Strategy

- Unit tests: template rendering with known context produces expected output
- Integration tests: run `modo new testapp --template <each>` in a temp dir, verify:
  - All expected files exist
  - `Cargo.toml` is valid TOML
  - `cargo check` passes on generated project
- Error case tests: missing args, conflicting flags, existing directory
