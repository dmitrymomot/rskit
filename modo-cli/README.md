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

| Option                      | Default | Description                                           |
| --------------------------- | ------- | ----------------------------------------------------- |
| `-t, --template <TEMPLATE>` | `web`   | Template preset to use                                |
| `--postgres`                | —       | Use PostgreSQL database driver                        |
| `--sqlite`                  | —       | Use SQLite database driver (default for DB templates) |
| `--s3`                      | —       | Use S3 storage with RustFS in development (web only)  |

`--postgres` and `--sqlite` are mutually exclusive.

### Templates

| Template  | Database          | Description                                                        |
| --------- | ----------------- | ------------------------------------------------------------------ |
| `minimal` | none              | Bare-bones project with configuration only                         |
| `api`     | sqlite / postgres | JSON API with handlers and models                                  |
| `web`     | sqlite / postgres | Full-stack web app with HTMX, jobs, email, auth, uploads, and i18n |
| `worker`  | sqlite / postgres | Background-job worker with no HTTP handlers                        |

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

Create a full-stack web app:

```bash
modo new my-app
# equivalent to: modo new my-app --template web --sqlite
```

Web app with PostgreSQL and S3 storage:

```bash
modo new my-app --postgres --s3
```

### After scaffolding

The CLI prints the recommended next steps. For a `web` project:

```
cd my-app
just assets-download     # download HTMX, Alpine.js (first time only)
just dev                 # start Docker, build CSS, run dev server
```

For other templates:

```
cd my-service
just dev
```

## Project name rules

- Must start with an ASCII letter or underscore (`[a-zA-Z_]`)
- May contain `[a-zA-Z0-9_-]`
- Must not be a Rust keyword

## What gets generated

Every project receives a shared `CLAUDE.md` (AI coding instructions for the project) plus template-specific files:

- `Cargo.toml` — pre-configured with the selected modo crates
- `src/main.rs` — application entry point
- `src/config.rs` — typed configuration struct
- `config/development.yaml` and `config/production.yaml`
- `.env` and `.env.example`
- `.gitignore`
- `justfile` — development task runner
- `docker-compose.yaml` — development services (Mailpit for email; PostgreSQL when `--postgres`; RustFS when `--s3`)

A `git init` is run automatically in the new directory after scaffolding.
