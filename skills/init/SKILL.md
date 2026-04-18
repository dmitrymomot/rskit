---
name: modo-init
allowed-tools: Read, Write, Glob, Grep, Bash, AskUserQuestion
description: "Scaffold a new modo v2 application — generates folder structure, Cargo.toml, config YAML, main.rs wiring, routes, handlers, migrations, justfile, Dockerfile, docker-compose, .env, .gitignore, and CI workflow. Use this skill whenever the user wants to create a new modo project, initialize an app, scaffold a starter, set up a new service, bootstrap a web app with modo, or says things like 'new project', 'init app', 'create service', 'start fresh', 'scaffold', or 'bootstrap'. Also use when the user asks how to set up a modo project from scratch."
---

# modo-init — Project Scaffolding

This skill scaffolds a complete, ready-to-run modo v2 application. It drives the conversation using `AskUserQuestion` to gather requirements, then creates files using bash scripts for static boilerplate and `Write` for dynamic assembly.

## Workflow

### Step 1: Project Basics

Use `AskUserQuestion` to ask these questions in a single call:

**Question 1 — App type preset:**
- header: "App type"
- question: "What kind of app are you building?"
- options:
  - **"API service (Recommended)"** — JSON API with auth, jobs, and RBAC. No HTML templates.
  - **"Web application"** — Server-rendered HTML with templates, auth, email, jobs, cron, and RBAC.
  - **"Full stack"** — Everything included — all 13 optional components enabled.
  - **"Custom"** — Pick individual components yourself.
- multiSelect: false

**Question 2 — Background processing:**
- header: "Jobs"
- question: "Do you need background job processing?"
- options:
  - **"Jobs + Cron (Recommended)"** — Background job queue with retries, plus async cron scheduler for recurring tasks.
  - **"Jobs only"** — Background job queue with retries and scheduling, no cron.
  - **"Neither"** — No background processing.
- multiSelect: false

After receiving answers, proceed based on the app type:

- **API service** → selected components: Auth, Jobs (per Q2), Cron (per Q2), RBAC. Go to Step 3.
- **Web application** → selected components: Templates, Auth, Email, Jobs (per Q2), Cron (per Q2), RBAC. Go to Step 3.
- **Full stack** → all 13 components selected. Go to Step 3.
- **Custom** → go to Step 2.

### Step 2: Custom Component Selection

Only reached if the user chose "Custom" in Step 1. Use `AskUserQuestion` with up to 4 multi-select questions in a single call:

**Question 1 — Frontend:**
- header: "Frontend"
- question: "Which frontend features do you need?"
- options:
  - **"Templates"** — MiniJinja HTML rendering + static file serving
  - **"SSE"** — Server-Sent Events with broadcaster pattern for real-time updates
- multiSelect: true

**Question 2 — Auth & access control:**
- header: "Auth"
- question: "Which auth and access control features do you need?"
- options:
  - **"Auth"** — Password hashing (Argon2), JWT (HS256), OAuth (GitHub/Google), TOTP, backup codes
  - **"RBAC"** — Role-based access control with guard layers
  - **"Multi-tenancy"** — Subdomain/domain/header/path-based tenant routing
- multiSelect: true

**Question 3 — Communication & integrations:**
- header: "Comms"
- question: "Which communication features do you need?"
- options:
  - **"Email"** — SMTP mailer with Markdown-to-HTML templates
  - **"Webhooks"** — Outbound webhook delivery with Standard Webhooks signing
  - **"DNS"** — Domain verification via TXT/CNAME records
- multiSelect: true

**Question 4 — Infrastructure:**
- header: "Infra"
- question: "Which infrastructure features do you need?"
- options:
  - **"Storage"** — S3-compatible object storage (AWS, MinIO, RustFS)
  - **"Geolocation"** — MaxMind GeoIP2 IP-to-location lookups
  - **"Sentry"** — Crash reporting and performance monitoring
- multiSelect: true

Collect all selected components and proceed to Step 3.

### Step 3: Confirm and Locate

Use `AskUserQuestion` to confirm details before generating:

**Question 1 — Project directory:**
- header: "Directory"
- question: "Where should I create the project?"
- options:
  - **"./"** — Current directory (files created here directly)
  - **"./<project-name>/"** — New subdirectory in current location
- multiSelect: false

Before asking, briefly list what will be generated:
- The core components (always included)
- The selected optional components
- Tell the user they can say "Other" to provide a custom path

### Step 4: Generate the Project

This step uses bash scripts for static boilerplate and `Write` for files that depend on component selection.

#### 4a: Run the scaffold script

Run `scripts/scaffold.sh` from this skill's directory to create the base project structure:

```bash
bash "<skill-dir>/scripts/scaffold.sh" "<project_dir>" "<project_name>"
```

This creates: directory structure, `src/config.rs`, `src/error.rs`, `src/routes/health.rs`, `migrations/app/001_initial.sql`, `.gitignore`, `.editorconfig`, `.github/workflows/ci.yml`, `data/.gitkeep`.

#### 4b: Run component scripts

For each selected component that has a script, run it in this order:

| Component | Script |
|-----------|--------|
| Templates | `bash "<skill-dir>/scripts/init_templates.sh" "<project_dir>"` |
| Templates | `bash "<skill-dir>/scripts/download_assets.sh" "<project_dir>"` |
| Jobs (or Cron) | `bash "<skill-dir>/scripts/init_jobs.sh" "<project_dir>"` |
| Email | `bash "<skill-dir>/scripts/init_email.sh" "<project_dir>"` |

Note: `download_assets.sh` downloads htmx, htmx-sse, and alpine.js. It runs after `init_templates.sh` which creates the `assets/static/js/` directory. `init_templates.sh` also compiles Tailwind CSS if the `tailwindcss` CLI is available.

#### 4c: Generate dynamic files

These files depend on the selected components — generate each with `Write`. Read `references/components.md` for the exact code blocks to assemble.

**Required dynamic files:**

1. **`Cargo.toml`** — modo ships every module unconditionally, so no per-module feature list is needed. See `references/files.md` for the template.

2. **`src/main.rs`** — Assemble from component blocks in `references/components.md`:
   - Module declarations and imports
   - Config loading and tracing init
   - Database connections and migrations
   - Service registry creation
   - Component initialization (one block per selected component)
   - Router construction with middleware stack
   - Background workers (if jobs/cron selected)
   - Server start and `run!` macro

3. **`src/routes/mod.rs`** — Core health route plus any component routes (e.g., home route if templates selected).

4. **`src/handlers/mod.rs`** — Module declarations for component handlers (e.g., `pub mod home;` if templates selected).

5. **`config/development.yaml`** — Core config sections plus component config sections from `references/components.md`.

6. **`config/production.yaml`** — Production variant of the config. Uses `${VAR}` (no defaults) for secrets.

7. **`.env.example`** — Core entries plus component-specific entries from `references/components.md`.

8. **`justfile`** — Assemble from base recipes + conditional recipe blocks based on selected components. See `references/files.md` for the full template with `command -v` guards on external tools. The `setup` recipe body is assembled conditionally (include `just assets-download` and `just css` if Templates, `just docker-up` if Docker services).

9. **`Dockerfile`** — Base template from `references/files.md`, plus conditional `COPY` lines:
   - If templates: `COPY templates/ /app/templates/`, `COPY assets/static/ /app/assets/static/`, `COPY locales/ /app/locales/`
   - If email: `COPY emails/ /app/emails/`

10. **`docker-compose.yml`** (only if Email or Storage selected) — Assemble service blocks from `references/components.md`.

11. **`src/tenant.rs`** (if Multi-tenancy selected) — Placeholder with `TenantResolver` trait docs.

12. **`src/rbac.rs`** (if RBAC selected) — Placeholder with `RoleExtractor` trait docs.

13. **`CLAUDE.md`** — Project context file for Claude Code. Assemble from `references/files.md` template, replacing `{{project_name}}` and conditional command/component sections based on selected components.

#### Middleware layering order

Applied bottom-up, executed top-down. Only include layers for selected components:

```rust
let app = routes::router(registry)
    // Static files (if templates)
    .merge(engine.static_service())
    // Error handling (innermost catch)
    .layer(modo::middleware::error_handler(handle_error))
    .layer(modo::middleware::catch_panic())
    // Observability
    .layer(modo::middleware::tracing())
    .layer(modo::middleware::request_id())
    // Response processing
    .layer(modo::middleware::compression())
    // Security
    .layer(modo::middleware::security_headers(&config.modo.security_headers)?)
    .layer(modo::middleware::cors(&config.modo.cors))
    .layer(modo::middleware::csrf(&config.modo.csrf, &cookie_key))
    // Template context (if templates)
    .layer(modo::template::TemplateContextLayer::new())
    // Session
    .layer(session_svc.layer())
    // Flash
    .layer(modo::flash::FlashLayer::new(cookie_config, &cookie_key))
    // Geolocation (if selected, must be after ClientIp)
    .layer(modo::geolocation::GeoLayer::new(geo_locator))
    // Client IP (outermost request-processing layer)
    .layer(modo::ip::ClientIpLayer::new())
    // Rate limiting
    .layer(rate_limit_layer);
```

### Step 5: Verify and Present

After generating all files:

1. List every file created with a brief description
2. Show the user how to get started:

```
cd <project>
just setup       # Copies .env, downloads assets, compiles CSS, starts Docker
just dev         # Run the app with auto-reload
```

3. Mention what they'll need to do next:
   - Write their own handlers in `src/handlers/`
   - Add routes in `src/routes/`
   - Create database migrations in `migrations/app/`
   - Implement `RoleExtractor` trait (if RBAC selected)
   - Implement `TenantResolver` trait (if multi-tenancy selected)
   - Register job handlers in main.rs (if jobs selected)

### Important Rules

- NEVER use absolute paths in generated code — everything is relative to the project root
- `mod.rs` and `lib.rs` files contain ONLY `mod` declarations and re-exports — all logic goes in separate files
- Use `modo::` prefix for all framework types (not bare imports from internal modules)
- The generated code must compile with `cargo check` — no placeholder `todo!()` stubs
- Generated handlers should be minimal but functional (return a simple response)
- Config YAML uses `${VAR:default}` for development, `${VAR}` (no default) for production
- Cookie secret must be at least 64 characters
- `.env.example` has safe defaults for development, no real secrets
- Rust edition is `2024`, rust-version is `"1.92"`
- HTML templates use ONLY Tailwind CSS utility classes — no custom CSS, no inline styles
- Static JS assets (htmx, alpine) are vendored in `assets/static/js/` and committed to the repo
- Justfile recipes that depend on external tools (`cargo-watch`, `tailwindcss`, `docker`) must guard with `command -v` checks

## References

- `references/components.md` — Code snippets for each component (registry, config, main.rs blocks)
- `references/files.md` — Boilerplate file templates (Cargo.toml, justfile, Dockerfile, CLAUDE.md, etc.)
