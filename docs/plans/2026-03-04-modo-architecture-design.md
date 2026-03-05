# modo — Architecture Document

## Context

Building a Rust web framework for micro-SaaS applications, inspired by Go's [forge](https://github.com/dmitrymomot/forge). Where forge embraces "no magic" with explicit wiring, modo embraces **"full magic" through Rust's compile-time capabilities** — proc macros, derives, generics, trait system — with zero runtime cost.

**Stack:** axum, SeaORM v2, SQLite-only, Askama, custom SQLite-backed job queue, `inventory` for auto-discovery.

**Target:** Solo devs / small teams who want Rails-like DX with Rust performance and single-binary deployment.

---

## 1. Philosophy

1. **Compile-time everything** — route registration, template rendering, job definitions, error responses, validation all verified at compile time
2. **Single binary deployment** — no external services; SQLite for data, jobs, sessions, rate limiting
3. **Convention over configuration** — sensible defaults; `#[modo::main]` wires the entire app automatically
4. **Progressive disclosure** — simple things are one line, complex things are possible via explicit opt-in

---

## 2. Crate Structure

Single crate with feature flags + mandatory `modo-macros` (proc macros must be separate in Rust).

```
modo/
  Cargo.toml                # Workspace root
  modo/                     # Main crate
    Cargo.toml
    src/
      lib.rs                # Public API, feature-gated re-exports
      app.rs                # App builder, lifecycle, graceful shutdown
      config.rs             # Config loading (env + .env + defaults)
      router.rs             # Route collection from inventory, router assembly
      handler.rs            # Handler trait, response types
      error.rs              # Error, content negotiation, IntoResponse
      middleware/
        mod.rs              # Middleware registry
        csrf.rs
        rate_limit.rs
        auth_guard.rs
      extractors/
        mod.rs
        db.rs               # Db extractor
        tenant.rs           # TenantId, TenantResolver, TenantScoped
        auth.rs             # Auth<User>, OptionalAuth<User>
        service.rs          # Service<T> extractor
        htmx.rs             # HtmxRequest extractor
        flash.rs            # Flash messages
        session.rs          # Session extractor
      db/
        mod.rs              # Database init, WAL mode, connection management
        migrations.rs       # Migration runner, auto-sync
        transaction.rs      # Transaction helpers
      tenancy/
        mod.rs              # TenantResolver trait, TenantScoped extension
        shared.rs           # SharedDatabase tenant_id scoping
      auth/
        mod.rs              # Traits: Authenticator, SessionStore, UserProvider
        password.rs         # PasswordAuthenticator (argon2)
        session.rs          # SqliteSessionStore
        oauth.rs            # OAuth client (Google/GitHub/custom)
        totp.rs             # TOTP 2FA
      jobs/
        mod.rs              # JobQueue, JobHandler trait
        schema.rs           # SQLite schema for jobs tables
        runner.rs           # Polling loop, concurrent execution
        scheduler.rs        # Cron/scheduled jobs
      templates/
        mod.rs              # BaseContext, HTMX helpers
        context.rs          # Template context injection
        htmx.rs             # HtmxResponse builder
      email/
        mod.rs              # Mailer, templated emails
      sse.rs                # Typed SSE event helpers
      webhooks.rs           # Outgoing webhooks with retries + HMAC
      storage/
        mod.rs              # StorageBackend trait
        s3.rs               # S3-compatible
        local.rs            # Local filesystem
      i18n.rs               # Locale detection, Fluent translations
      validation.rs         # Validated<T> extractor, HTML sanitization
      jwt.rs                # JWT create/verify/decode
      test_helpers.rs       # TestApp, assertions, email capture
  modo-macros/             # Proc macro crate
    Cargo.toml
    src/
      lib.rs                # Macro entry points
      handler.rs            # #[handler(GET, "/path")]
      job.rs                # #[job(...)] and #[derive(Job)]
      middleware.rs          # #[middleware(...)]
      module.rs             # #[module(prefix, middleware)]
      entity.rs             # #[derive(Entity)]
      error.rs              # #[derive(IntoResponse)] with #[status(N)]
      main.rs               # #[modo::main]
```

### Feature Flags

```toml
[features]
default = ["auth", "templates", "jobs", "sessions"]
auth = ["sessions"]
templates = ["dep:askama"]
jobs = []
sessions = []
db = ["dep:sea-orm"]
tenancy = ["db"]
email = ["dep:lettre", "templates", "jobs"]
sse = []
webhooks = ["jobs"]
storage = ["dep:aws-sdk-s3"]
i18n = ["dep:fluent"]
csrf = ["sessions"]
rate-limiting = []
jwt = ["dep:jsonwebtoken"]
validation = ["dep:validator"]
totp = ["dep:totp-rs"]
oauth = ["dep:oauth2"]
test-helpers = []
```

---

## 3. Core Architecture

### App Builder

```rust
pub struct AppBuilder {
    config: AppConfig,
    services: Vec<Box<dyn Any + Send + Sync>>,
    middleware: Vec<BoxLayer>,
    on_startup: Vec<Box<dyn FnOnce(&App) -> BoxFuture<'_, Result<()>>>>,
    on_shutdown: Vec<Box<dyn FnOnce(&App) -> BoxFuture<'_, Result<()>>>>,
}

impl AppBuilder {
    pub fn new() -> Self { ... }
    pub fn service<T: Send + Sync + 'static>(self, svc: T) -> Self { ... }  // wrapped in Arc
    pub fn layer<L: Layer<Route>>(self, layer: L) -> Self { ... }
    pub fn database(self, url: &str) -> Self { ... }
    pub fn tenancy(self, strategy: TenantStrategy) -> Self { ... }
    pub fn on_startup<F>(self, f: F) -> Self { ... }
    pub fn on_shutdown<F>(self, f: F) -> Self { ... }
    pub async fn run(self) -> Result<()> { ... }
}
```

### Lifecycle

1. `#[modo::main]` expands, collects all auto-discovered routes/jobs/modules via `inventory`
2. User calls `.service()`, `.layer()`, etc.
3. `.run()`: init DB (WAL mode) -> schema sync (framework + user entities) -> run pending migrations -> build Router from inventory -> apply middleware -> start job workers -> execute startup hooks -> serve with graceful shutdown

### Configuration

```rust
pub struct AppConfig {
    pub bind_address: SocketAddr,    // default "0.0.0.0:3000"
    pub database_url: String,        // default "sqlite://data.db?mode=rwc"
    pub secret_key: SecretKey,       // REQUIRED in production
    pub environment: Environment,    // Development | Production | Test
    pub log_level: tracing::Level,
    pub max_body_size: usize,
    pub job_concurrency: usize,
    pub job_poll_interval: Duration,
}
```

Loaded from env vars with `MODO_` prefix, then `.env` file, then defaults.

---

## 4. Macro System — Auto-Discovery via `inventory`

**Why `inventory` over `linkme`:** `linkme` uses linker sections with a [known issue](https://github.com/dtolnay/linkme/issues/36) where distributed slice members in dependency crates are silently discarded. `inventory` uses pre-main global constructors — reliable across platforms including WASM.

### Registration Types

```rust
pub struct RouteRegistration {
    pub method: Method,
    pub path: &'static str,
    pub handler: fn() -> MethodRouter<AppState>,
    pub middleware: Vec<fn() -> BoxLayer>,
    pub module: Option<&'static str>,
}
inventory::collect!(RouteRegistration);

pub struct JobRegistration {
    pub name: &'static str,
    pub queue: &'static str,
    pub max_retries: u32,
    pub timeout: Duration,
    pub cron: Option<&'static str>,
    pub handler_factory: fn() -> Box<dyn JobHandler>,
}
inventory::collect!(JobRegistration);

pub struct ModuleRegistration {
    pub prefix: &'static str,
    pub middleware: Vec<fn() -> BoxLayer>,
}
inventory::collect!(ModuleRegistration);
```

### `#[handler(GET, "/path")]`

User writes:

```rust
#[handler(GET, "/users/:id")]
async fn get_user(Path(id): Path<i64>, Db(db): Db) -> Result<Json<User>> {
    let user = UserEntity::find_by_id(id).one(&db).await?;
    Ok(Json(user.ok_or(AppError::NotFound)?))
}
```

Macro expands to:

```rust
async fn get_user(Path(id): Path<i64>, Db(db): Db) -> Result<Json<User>> { ... }

inventory::submit! {
    modo::RouteRegistration {
        method: modo::Method::GET,
        path: "/users/:id",
        handler: || axum::routing::get(get_user),
        middleware: vec![],
        module: None,
    }
}
```

### `#[middleware(my_fn(params))]` on handlers

```rust
#[handler(POST, "/users")]
#[middleware(require_auth())]
#[middleware(rate_limit(10, Duration::from_secs(60)))]
async fn create_user(Json(input): Json<CreateUser>, Db(db): Db) -> Result<Json<User>> { ... }
```

Middleware functions are plain async functions:

```rust
async fn rate_limit(req: Request, next: Next, max_requests: u32, window: Duration) -> Result<Response> {
    // check rate limit...
    Ok(next.run(req).await)
}
```

The macro wraps them into Tower layers via `axum::middleware::from_fn`.

### `#[module(prefix, middleware)]`

```rust
#[module(prefix = "/admin", middleware = [require_role("admin")])]
mod admin {
    #[handler(GET, "/dashboard")]
    async fn dashboard(auth: Auth<User>) -> impl IntoResponse { ... }
}
```

Registers `ModuleRegistration` via inventory. Inner `#[handler]` macros get `module: Some("admin")`. Router builder nests module routes under prefix with module middleware.

### `#[job(...)]`

```rust
#[job(queue = "emails", max_retries = 3, timeout = "30s")]
async fn send_welcome_email(payload: WelcomeEmailPayload, mailer: Service<Mailer>) -> Result<()> { ... }
```

Generates: `SendWelcomeEmailJob` struct with `.enqueue()` and `.enqueue_in()` methods, plus `inventory::submit!` for `JobRegistration`.

### `#[modo::main]`

```rust
#[modo::main]
async fn main(app: AppBuilder) -> Result<()> {
    app.service(Mailer::new(config.smtp_url))
       .run()
       .await
}
```

Expands to `#[tokio::main]` + config loading + collecting all routes/jobs/modules from `inventory` + calling user's body.

---

## 5. Routing

Routes auto-registered via `#[handler]`. Router built at startup by iterating `inventory::iter::<RouteRegistration>`:

- Group routes by module
- Build module-less routes directly on root router
- Nest module routes under prefix with module middleware
- Apply per-handler middleware via Tower layers

Execution order: Global layers (outermost) -> Module middleware -> Handler middleware (innermost).

### AppState

```rust
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub services: ServiceRegistry,
    pub job_queue: JobQueue,
    pub config: AppConfig,
    pub session_store: Option<Arc<dyn SessionStore>>,
}
```

---

## 6. Database Layer

### SQLite initialization with performance pragmas:

```rust
db.execute_unprepared("PRAGMA journal_mode=WAL").await?;
db.execute_unprepared("PRAGMA busy_timeout=5000").await?;
db.execute_unprepared("PRAGMA synchronous=NORMAL").await?;
db.execute_unprepared("PRAGMA foreign_keys=ON").await?;
```

### Db extractor

```rust
pub struct Db(pub DatabaseConnection);
// Extracts from AppState, implements FromRequestParts
```

### Transaction helper

```rust
pub async fn transaction<F, Fut, T>(db: &DatabaseConnection, f: F) -> Result<T>
where F: FnOnce(DatabaseTransaction) -> Fut, Fut: Future<Output = Result<T>>
{
    let txn = db.begin().await?;
    match f(txn).await {
        Ok(result) => { txn.commit().await?; Ok(result) }
        Err(e) => { txn.rollback().await?; Err(e) }
    }
}
```

### Migrations — Entity-First with Auto-Sync

**Full design:** `docs/plans/2026-03-05-entity-first-migrations-design.md`

**Approach:** Entity-first — define Rust structs with `#[modo::entity]`, framework auto-syncs schema on startup. Hybrid escape hatches via `#[modo::migration]` for destructive changes and data migrations.

**Key points:**
- `#[modo::entity]` replaces `DeriveEntityModel` — generates SeaORM derives, relations, indices, and `inventory` registration in one macro
- Extended attributes beyond SeaORM: `on_delete`, `on_update`, composite `index(columns = [...])`, `renamed_from`
- SeaORM v2 `schema-sync` runs on every startup (all environments) — addition-only, never drops tables/columns
- Framework entities (`_modo_sessions`, `_modo_jobs`, etc.) merge with user entities in a single sync pass
- Framework tables are `pub` and queryable but framework-owned
- `#[modo::migration(version = N)]` for data migrations, auto-discovered via `inventory`
- Migration history tracked in `_modo_migrations` table

---

## 7. Multi-Tenancy (Shared Database)

Shared-database strategy only. Per-database tenancy (LRU pool of separate SQLite files) deferred to Phase 5 as an advanced module.

### Tenant Resolution

```rust
pub trait TenantResolver: Send + Sync + 'static {
    async fn resolve(&self, req: &Parts) -> Result<TenantId, AppError>;
}
// Built-in: SubdomainResolver, PathPrefixResolver, HeaderResolver, UserTenantResolver
```

### SharedDatabase Scoping

Extension trait `TenantScoped` auto-adds `WHERE tenant_id = ?` to SeaORM queries. Tenant ID injected into request extensions by middleware, accessible via `TenantId` extractor.

### TenantId Extractor

```rust
pub struct TenantId(pub String);
// Extracted from request extensions (set by tenant resolution middleware).
// Handlers use this to scope queries via TenantScoped trait.
```

---

## 8. Session & Auth

### Layered Traits

```rust
pub trait Authenticator: Send + Sync + 'static {
    type User: Clone + Send + Sync + 'static;
    type Credentials;
    async fn authenticate(&self, credentials: Self::Credentials) -> Result<Self::User, AuthError>;
}

pub trait UserProvider: Send + Sync + 'static {
    type User: Clone + Send + Sync + 'static;
    type UserId: Clone + Send + Sync + 'static;
    async fn find_by_id(&self, id: &Self::UserId) -> Result<Option<Self::User>, AuthError>;
}

pub trait SessionStore: Send + Sync + 'static {
    async fn create(&self, data: SessionData) -> Result<SessionId, AuthError>;
    async fn read(&self, id: &SessionId) -> Result<Option<SessionData>, AuthError>;
    async fn update(&self, id: &SessionId, data: SessionData) -> Result<(), AuthError>;
    async fn destroy(&self, id: &SessionId) -> Result<(), AuthError>;
    async fn destroy_all_for_user(&self, user_id: &str) -> Result<(), AuthError>;
    async fn cleanup_expired(&self) -> Result<u64, AuthError>;
}
```

### Default Implementations

- `PasswordAuthenticator` — argon2 password hashing/verification
- `SqliteSessionStore` — session persistence in SQLite, configurable TTL, max sessions per user

### Extractors

- `Auth<User>` — returns 401 if not authenticated
- `OptionalAuth<User>` — returns `None` instead of 401

### Session Middleware

Global middleware loads session from cookie on every request, injects into request extensions. Token rotation on authentication.

---

## 9. Background Jobs

Custom SQLite-backed queue (not Apalis) for tighter integration.

### Schema

```sql
CREATE TABLE modo_jobs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    queue TEXT NOT NULL DEFAULT 'default',
    payload TEXT NOT NULL DEFAULT '{}',
    state TEXT NOT NULL DEFAULT 'pending',  -- pending|running|completed|failed|dead
    priority INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    run_at TEXT NOT NULL DEFAULT (datetime('now')),
    timeout_secs INTEGER NOT NULL DEFAULT 300,
    dedupe_key TEXT,
    tenant_id TEXT,
    last_error TEXT,
    locked_by TEXT,
    locked_at TEXT,
    ...
);

-- NOTE: Cron/scheduled jobs are in-memory only (tokio timers).
-- No database persistence for cron schedule state.
-- Errors logged via tracing, not stored in DB.
```

### JobQueue API

```rust
impl JobQueue {
    pub async fn enqueue<T: Serialize>(&self, name: &str, payload: T) -> Result<JobId>;
    pub async fn enqueue_at<T: Serialize>(&self, name: &str, payload: T, run_at: DateTime<Utc>) -> Result<JobId>;
    pub async fn enqueue_in_txn<T: Serialize>(&self, txn: &DatabaseTransaction, name: &str, payload: T) -> Result<JobId>;
    pub async fn enqueue_unique<T: Serialize>(&self, name: &str, payload: T, dedupe_key: &str) -> Result<JobId>;
}
```

### Job Runner

Polling loop with configurable interval. Semaphore-bounded concurrency. Atomic poll (SELECT + UPDATE). Exponential backoff on retry (5s, 10s, 20s, 40s... capped at 1hr). Stale lock reaping for crashed workers. Graceful shutdown drains in-flight jobs.

### Job Lifecycle

```
pending --[polled]--> running --[success]--> completed
                         |
                         +--[failure, retries left]--> pending (with backoff)
                         +--[failure, no retries]--> dead
```

---

## 10. Templating & HTMX

### BaseContext (auto-extracted)

```rust
pub struct BaseContext {
    pub is_htmx: bool,
    pub current_url: String,
    pub flash_messages: Vec<FlashMessage>,
    pub csrf_token: String,
    pub current_user: Option<serde_json::Value>,
    pub locale: String,
}
// Implements FromRequestParts — available as handler parameter
```

### Template Pattern

```rust
#[derive(Template)]
#[template(path = "users/show.html")]
pub struct UserShowTemplate {
    pub base: BaseContext,
    pub user: User,
}

#[handler(GET, "/users/:id")]
async fn show_user(Path(id): Path<i64>, Db(db): Db, base: BaseContext) -> Result<UserShowTemplate> {
    let user = UserEntity::find_by_id(id).one(&db).await?.ok_or(AppError::NotFound)?;
    Ok(UserShowTemplate { base, user })
}
```

### HTMX Dual Rendering

Templates use `{% if base.is_htmx %}` for conditional rendering. Or handlers return different types based on `base.is_htmx`.

### HTMX Response Helpers

```rust
pub struct HtmxResponse { ... }
impl HtmxResponse {
    pub fn redirect(url: &str) -> Self;      // HX-Redirect header
    pub fn trigger(self, event: &str) -> Self; // HX-Trigger header
    pub fn push_url(self, url: &str) -> Self;  // HX-Push-Url header
    pub fn reswap(self, strategy: &str) -> Self;
    pub fn retarget(self, selector: &str) -> Self;
}
```

---

## 11. Error Handling

### Derive-based with status codes

```rust
#[derive(Debug, Error, IntoResponse)]
pub enum AppError {
    #[error("Not found")]
    #[status(404)]
    NotFound,

    #[error("Unauthorized")]
    #[status(401)]
    Unauthorized,

    #[error("Validation error")]
    #[status(422)]
    Validation(ValidationErrors),

    #[error(transparent)]
    #[status(500)]
    Internal(#[from] anyhow::Error),
}
```

### Content Negotiation

`#[derive(IntoResponse)]` generates impl that auto-detects:

- HTMX request -> render error as partial template
- JSON request (Accept: application/json) -> `{"error": "...", "status": N}`
- Browser request -> render full error page

Framework provides `Error` base enum for common cases. Users define their own domain errors.

---

## 12. Additional Modules (all behind feature flags)

| Module            | Description                                                                                                   |
| ----------------- | ------------------------------------------------------------------------------------------------------------- |
| **SSE**           | Typed `SseEvent` trait, `TypedSse<T>` response type on top of axum's native SSE                               |
| **Webhooks**      | Outgoing webhook delivery via job queue, HMAC-SHA256 signatures, exponential backoff retries, circuit breaker |
| **File Storage**  | `StorageBackend` trait with S3 and local FS implementations, multi-tenant path builder                        |
| **i18n**          | Fluent-based translations, locale detection middleware, Askama filter `{{ "key"\|t(locale) }}`                |
| **CSRF**          | Double-submit cookie pattern, auto-injected token in BaseContext                                              |
| **Rate Limiting** | Per-IP/user/endpoint, SQLite-backed (persistent) or in-memory (fast), `RateLimiter` trait                     |
| **JWT**           | Create/verify/decode, configurable algorithms, `JwtConfig`                                                    |
| **Validation**    | `Validated<T>` extractor (deserialize + validate in one step), HTML sanitization via ammonia                  |
| **TOTP 2FA**      | RFC 6238, AES-256-GCM encrypted secrets, recovery codes, QR code generation                                   |
| **OAuth Client**  | `OAuthProvider` trait, Google/GitHub/custom implementations, authorization code flow                          |
| **Email**         | SMTP via lettre, Askama-templated emails, queue via job system, `TestMailer` captures in memory               |

---

## 13. Testing Framework

```rust
let app = TestApp::builder()
    .as_user(admin_user())
    .with_role("admin")
    .build()
    .await;

let response = app.post("/api/users")
    .json(&json!({ "name": "Alice", "email": "alice@example.com" }))
    .send()
    .await;

response.assert_ok();
let user: User = response.json();
assert_eq!(user.name, "Alice");
app.assert_email_sent_to("alice@example.com");

// HTMX testing
let response = app.get("/users").htmx().send().await;
response.assert_select(".user-list").assert_count(1);
response.assert_hx_trigger("users-loaded");
```

Features: in-memory SQLite, fake auth, request builders, CSS selector assertions (scraper crate), HTMX assertions, email capture.

---

## 14. Implementation Phases

### Phase 1: Foundation

- `modo-macros`: `#[handler]`, `#[modo::main]`
- `inventory`-based auto-discovery
- `AppBuilder` with config, graceful shutdown
- `Db` extractor, SeaORM v2 + SQLite + WAL mode
- `Error` with content negotiation
- `Service<T>` extractor

**Milestone:** `#[handler(GET, "/")] async fn index() -> &'static str { "Hello modo" }` works with `#[modo::main]`.

### Phase 2: Auth, Sessions, Templates

- `SqliteSessionStore`, auth traits + default impls
- `Auth<User>`, `OptionalAuth<User>` extractors
- Askama + `BaseContext` + HTMX auto-detection
- Flash messages, CSRF middleware
- `#[middleware]` and `#[module]` macros

**Milestone:** Login/register flow with HTMX partial rendering and CSRF protection.

### Phase 3: Jobs, Shared-DB Multi-Tenancy

- SQLite job queue: schema, polling, retries, cron, dedup, transactional enqueue
- `#[job]` macro
- Shared-database multi-tenancy: `TenantResolver` trait, `TenantId` extractor, `TenantScoped` query extension
- Tenant resolution middleware

**Milestone:** Job enqueued in a transaction executes after commit. Tenant-scoped queries work transparently.

### Phase 4: Email, Testing, DX

- Mailer + Askama-templated emails + queue via jobs
- `TestApp` builder, fake auth, request builders, HTML assertions
- `#[derive(Entity)]` wrapper, `Validated<T>` extractor
- Rate limiting middleware

**Milestone:** Full integration test with auth, templates, jobs, and email in one test.

### Phase 5: Advanced Modules

- SSE, webhooks, file storage, i18n, JWT, TOTP, OAuth client, HTML sanitization
- Per-database multi-tenancy (LRU connection pool, sharded dirs, cross-tenant migrations)
- Documentation and example apps
- Optional CLI scaffolding tool

**Milestone:** Complete micro-SaaS example app with all features.

---

## 15. Key Dependencies

```toml
# Core
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = "0.6"
serde = { version = "1", features = ["derive"] }
inventory = "0.3"
anyhow = "1"
thiserror = "2"
tracing = "0.1"

# Database
sea-orm = { version = "2", features = ["sqlx-sqlite", "runtime-tokio-rustls"] }

# Templates
askama = "0.13"
askama_axum = "0.5"

# Auth
argon2 = "0.5"

# Jobs
chrono = { version = "0.4", features = ["serde"] }

# Multi-tenancy
lru = "0.12"
dashmap = "6"

# Additional (optional)
lettre = "0.11"          # email
jsonwebtoken = "9"       # jwt
validator = "0.18"       # validation
totp-rs = "6"            # totp
oauth2 = "5"             # oauth
aws-sdk-s3 = "1"         # storage
fluent = "0.16"          # i18n
ammonia = "4"            # sanitization
scraper = "0.21"         # test HTML assertions
```

---

## 16. Design Decision Summary

| Decision          | Choice                        | Rationale                                                         |
| ----------------- | ----------------------------- | ----------------------------------------------------------------- |
| Auto-discovery    | `inventory` over `linkme`     | No linker section cross-crate issues                              |
| Job queue         | Custom over Apalis            | Tighter transactional enqueue + multi-tenancy integration         |
| Middleware model  | Plain async functions         | Much simpler than Tower Layer+Service for 90% case                |
| Error handling    | Derive macro with `#[status]` | Declarative, auto content negotiation                             |
| Multi-tenancy     | Shared-DB first, per-DB later | Simpler to operate/backup; per-DB deferred to Phase 5             |
| Template context  | `BaseContext` extractor       | Auto HTMX/flash/CSRF injection                                    |
| Session backend   | SQLite only                   | Single-binary philosophy                                          |
| Test approach     | In-memory SQLite              | Fast, isolated, real DB behavior                                  |
| Service DI        | Manual construction           | Explicit, no magic DI container                                   |
| Auth              | Layered traits + defaults     | Swappable components, works for both OAuth server and client apps |

---

## Verification

After Phase 1 implementation:

1. `cargo build` — verifies proc macros compile
2. `cargo test` — runs unit tests with in-memory SQLite
3. Run example app: `cargo run --example hello` — confirms handler auto-registration, DB connection, graceful shutdown
4. Verify: `curl http://localhost:3000/` returns expected response
5. Verify: `curl http://localhost:3000/users/1` returns JSON with correct content negotiation
6. Kill with SIGTERM — verify graceful shutdown completes
