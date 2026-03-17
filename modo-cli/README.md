# modo-cli

CLI tool for scaffolding new [modo](https://github.com/dmitrymomot/modo) framework projects.

## Installation

```bash
cargo install modo-cli
```

After installation the `modo` binary is available on your `$PATH`.

## Usage

### Create a new project

```
modo new <NAME> [OPTIONS]
```

| Option                      | Default  | Description                                           |
| --------------------------- | -------- | ----------------------------------------------------- |
| `-t, --template <TEMPLATE>` | `web`    | Template preset to use                                |
| `--postgres`                | —        | Use PostgreSQL database driver                        |
| `--sqlite`                  | —        | Use SQLite database driver (default for DB templates) |
| `--s3`                      | —        | Use S3 storage with RustFS in development (web only)  |

`--postgres` and `--sqlite` are mutually exclusive.

### Templates

| Template  | Database          | Description                                                             |
| --------- | ----------------- | ----------------------------------------------------------------------- |
| `minimal` | none              | Bare-bones project with configuration only, no database                 |
| `api`     | sqlite / postgres | JSON API with handlers and models                                       |
| `web`     | sqlite / postgres | Full-stack web app with HTMX, Tailwind CSS, jobs, email, auth, and i18n |
| `worker`  | sqlite / postgres | Background-job worker with no HTTP handlers                             |

### Examples

Create a minimal project (no database):

```bash
modo new my-service --template minimal
```

Create a JSON API project backed by SQLite (default):

```bash
modo new my-api --template api
```

Create a JSON API project backed by PostgreSQL:

```bash
modo new my-api --template api --postgres
```

Create a full-stack web app (default template, SQLite):

```bash
modo new my-app
```

Web app with PostgreSQL and S3 storage:

```bash
modo new my-app --postgres --s3
```

### After scaffolding

The CLI prints the recommended next steps. For a `web` project:

```
cd my-app
just assets-download     # download HTMX, Alpine.js, Tailwind Elements (first time only)
just dev                 # start dev server
```

For other templates (`api`, `worker`, `minimal`):

```
cd my-service
just dev
```

`just dev` starts any configured Docker Compose services (Postgres via `--postgres`, RustFS via
`--s3`, or Mailpit in `web` templates) before launching the dev server.

## Project name rules

- Must start with an ASCII letter or underscore (`[a-zA-Z_]`)
- May contain `[a-zA-Z0-9_-]`
- Must not be a Rust keyword

## What gets generated

Every project receives shared files plus template-specific files. Shared files applied to all templates:

- `.gitignore`
- `CLAUDE.md` — AI coding instructions for the project

Template-specific files (vary by template):

- `Cargo.toml` — pre-configured with the selected modo crates
- `src/main.rs` — application entry point
- `src/config.rs` — typed configuration struct
- `config/development.yaml` and `config/production.yaml`
- `.env` and `.env.example`
- `justfile` — development task runner

The `api` and `web` templates also generate `src/handlers/` and `src/models/`. The `web` template
additionally generates `src/views/`, `src/tasks/`, `assets/`, `templates/`, and `locales/`. The
`worker` template generates `src/tasks/`.

The `web` and `worker` templates create a `data/` directory (with a `.gitkeep`) as the default
SQLite database location.

A `docker-compose.yaml` is generated when it has content to include:

- `web` template: always generated (always includes Mailpit for email); adds Postgres when
  `--postgres`; adds RustFS when `--s3`
- `api` and `worker` templates: only generated when `--postgres` is passed (Postgres service only)
- `minimal` template: never generated

A `git init` is run automatically in the new directory after scaffolding.

## Template variables

Templates are rendered with [MiniJinja](https://docs.rs/minijinja). The scaffold-time variables
available in `.jinja` files are:

| Variable       | Type    | Description                                                   |
| -------------- | ------- | ------------------------------------------------------------- |
| `project_name` | string  | The project name passed to `modo new`                         |
| `db_driver`    | string  | `"sqlite"`, `"postgres"`, or `""` for templates without a DB |
| `s3`           | boolean | `true` when `--s3` is passed                                  |

Email templates under `templates/emails/` use `{% raw %}...{% endraw %}` blocks to preserve
runtime Jinja variables (such as `{{name}}` and `{{subject}}`) so they are not consumed during
scaffolding.
