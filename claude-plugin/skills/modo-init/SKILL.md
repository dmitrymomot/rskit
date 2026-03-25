---
name: modo-init
description: "Scaffold a new modo v2 application — generates folder structure, Cargo.toml, config YAML, main.rs wiring, routes, handlers, migrations, justfile, Dockerfile, docker-compose, .env, .gitignore, and CI workflow. Use this skill whenever the user wants to create a new modo project, initialize an app, scaffold a starter, set up a new service, bootstrap a web app with modo, or says things like 'new project', 'init app', 'create service', 'start fresh', 'scaffold', or 'bootstrap'. Also use when the user asks how to set up a modo project from scratch."
---

# modo-init — Project Scaffolding

This skill generates a complete, ready-to-run modo v2 application. It drives the conversation using `AskUserQuestion` to gather requirements, then creates every file with proper wiring.

## Workflow

You MUST use the `AskUserQuestion` tool at each step to gather requirements. Do NOT dump text menus or expect freeform answers — use structured questions with options so the user can click through choices quickly.

### Step 1: Project Basics

Use `AskUserQuestion` to ask the first round of questions. Ask up to 4 questions in a single call:

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

Only reached if the user chose "Custom" in Step 1. Use `AskUserQuestion` to ask up to 4 multi-select questions in a single call:

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
  - **"./\<project-name\>/"** — New subdirectory in current location
- multiSelect: false

Before asking, briefly list what will be generated:
- The core components (always included)
- The selected optional components
- Tell the user they can say "Other" to provide a custom path

After this answer, you have everything needed. Proceed to generate.

### Step 4: Generate the Project

Read `references/components.md` for the exact code to generate for each component. Read `references/files.md` for boilerplate file templates.

The project name is derived from the target directory name (last path segment), converted to a valid Rust crate name (lowercase, hyphens).

The generated project structure follows this layout:

```
<project>/
├── Cargo.toml
├── justfile
├── Dockerfile
├── docker-compose.yml      (if storage or email selected)
├── .env.example
├── .gitignore
├── .github/workflows/ci.yml
├── config/
│   ├── development.yaml
│   └── production.yaml
├── data/
│   └── .gitkeep
├── migrations/
│   ├── app/
│   │   └── 001_initial.sql
│   └── jobs/                (if jobs selected)
│       └── 001_jobs.sql
├── src/
│   ├── main.rs
│   ├── config.rs
│   ├── error.rs             (custom error handler)
│   ├── routes/
│   │   ├── mod.rs
│   │   └── health.rs
│   ├── handlers/
│   │   └── mod.rs
│   └── jobs/                (if jobs selected)
│       ├── mod.rs
│       └── example.rs
├── templates/               (if templates selected)
│   ├── base.html
│   └── home.html
├── emails/                  (if email selected)
│   └── welcome.md
└── static/                  (if templates selected)
    └── .gitkeep
```

#### File Generation Rules

**`src/main.rs`** is the most critical file — it wires everything together. Build it by assembling blocks from `references/components.md` in this order:

1. Module declarations and imports
2. Config loading and tracing init
3. Database connections and migrations
4. Service registry creation
5. Component initialization (one block per selected component)
6. Router construction with middleware stack
7. Background workers (if jobs/cron selected)
8. Server start and `run!` macro

**Middleware layering order** matters (applied bottom-up, executed top-down). Always use this order for the layers that are present:

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
    .layer(modo::middleware::security_headers(&config.modo.security_headers))
    .layer(modo::middleware::cors(&config.modo.cors))
    .layer(modo::middleware::csrf(&config.modo.csrf, &cookie_key))
    // Template context (if templates)
    .layer(modo::TemplateContextLayer::new(engine))
    // Session
    .layer(modo::session::layer(session_store, cookie_config, &cookie_key))
    // Flash
    .layer(modo::FlashLayer::new(cookie_config, &cookie_key))
    // Geolocation (if selected, must be after ClientIp)
    .layer(modo::GeoLayer::new(geo_locator))
    // Client IP (outermost request-processing layer)
    .layer(modo::ClientIpLayer::new())
    // Rate limiting
    .layer(rate_limit_layer);
```

Only include layers for selected components. If templates aren't selected, omit `TemplateContextLayer` and `static_service()`. If geolocation isn't selected, omit `GeoLayer`. And so on.

**Config YAML** — include only sections for selected components. Always include: `server`, `database`, `tracing`, `cookie`, `session`, `rate_limit`, `trusted_proxies`, `cors`.

**Cargo.toml features** — map selected components to modo feature flags:
- Templates → `"templates"`
- Auth → `"auth"`
- Email → `"email"`
- Storage → `"storage"`
- SSE → `"sse"`
- Webhooks → `"webhooks"`
- DNS → `"dns"`
- Geolocation → `"geolocation"`
- Sentry → `"sentry"`
- Jobs, Cron, Multi-tenancy, RBAC → no feature flag needed (always available)

If ALL feature-gated components are selected, use `features = ["full"]` instead of listing them individually.

**docker-compose.yml** — include services based on selections:
- Email → Mailpit (SMTP + web UI)
- Storage → RustFS (S3-compatible) + bucket init sidecar

Only create docker-compose.yml if at least one docker service is needed.

### Step 5: Verify and Present

After generating all files:

1. List every file created with a brief description
2. Show the user how to get started:

```
cd <project>
cp .env.example .env
# Edit .env with your secrets
just services    # (if docker-compose exists) Start local services
just dev         # Run the app
```

3. Mention what they'll need to do next:
   - Write their own handlers in `src/handlers/`
   - Add routes in `src/routes/`
   - Create database migrations in `migrations/app/`
   - Implement `RoleExtractor` trait (if RBAC selected)
   - Implement `TenantResolver` trait (if multi-tenancy selected)
   - Register job handlers in main.rs (if jobs selected)
   - Download MaxMind GeoLite2-City.mmdb (if geolocation selected)

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

## References

- `references/components.md` — Code snippets for each component (registry, config, main.rs blocks)
- `references/files.md` — Boilerplate file templates (Cargo.toml, justfile, Dockerfile, etc.)
