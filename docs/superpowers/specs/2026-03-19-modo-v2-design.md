# modo v2 — Design Specification

## Philosophy

One crate. Zero proc macros. Plain functions. Explicit wiring. Moderate magic through extractors and traits, not code generation.

modo v2 is a clean rewrite — a collection of small, testable modules inside a single crate that help build Rust web applications. Each module is independent, explicitly wired by the user in `main()`. No global state, no auto-registration, no hidden orchestration.

**Target audience:** Rust developers building small monolithic web apps — primarily B2B SaaS, occasionally B2C. Optimized for solo developer productivity with enough ergonomics that developers from Rails/Django/Express don't bounce off Rust's complexity.

**Key design decisions:**
- Handlers are plain `async fn` — no `#[handler]` macro, no signature rewriting
- Routes use axum's `Router` directly — no auto-registration, no `inventory`
- Services are wired explicitly in `main()` — no global discovery
- Database uses raw sqlx — no ORM, no `Record` trait, no `ActiveModel`
- All config structs have sensible `Default` implementations
- Feature flags only for truly optional pieces (templates, SSE, OAuth)

## Crate Structure

```
dmitrymomot/modo/
  Cargo.toml
  src/
    lib.rs
    config/          -- YAML config loading with env var substitution
    db/              -- sqlx connection, read/write split, pool management
    error/           -- Error, Result, HttpError
    extractor/       -- JsonRequest, FormRequest, MultipartRequest, Query, Path, Service
    validate/        -- Validate trait, builder API, validation rules
    sanitize/        -- Sanitize trait, string operations
    server/          -- HTTP server
    service/         -- Registry (type-map for dependency injection)
    runtime/         -- Task orchestration, signal handling, sequential shutdown
    middleware/      -- cors, csrf, rate-limit, request-id, tracing, security headers, compression
    cookie/          -- signed/encrypted cookie manager
    session/         -- DB-backed sessions with token hashing, fingerprinting
    auth/            -- guards, password hashing (Argon2id), TOTP, OTP, backup codes
    oauth/           -- Google, GitHub OAuth flows (feature-gated)
    tenant/          -- tenant resolution with custom resolve function
    template/        -- MiniJinja engine, i18n, static files
    sse/             -- broadcast manager, imperative channels
    job/             -- DB-backed queue, enqueuer, worker
    cron/            -- in-memory recurring task scheduler
    email/           -- SMTP transport, markdown templates, layout engine
    upload/          -- S3-compatible storage, multipart extraction
    test/            -- TestApp, TestClient, in-memory DB, fixtures
  tests/
  README.md
```

**Feature flags** (all on by default via `full`):
- `sqlite` (default) / `postgres` — mutually exclusive DB backend (enforced via `compile_error!` if both enabled)
- `templates` — MiniJinja + i18n + static files
- `sse` — broadcast SSE
- `oauth` — Google/GitHub OAuth (pulls reqwest)

Everything else is always-on.

**Companion crate:**
- `modo-cli` — project scaffolding CLI (`modo new my-app`). Separate binary crate, no runtime dependency on the framework. Generates project structure, config files, example module, migrations for sessions/jobs. Design TBD — will be specified separately.

## App Bootstrap

```rust
use modo::{config, db, server, service};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::load::<AppConfig>("config/")?;
    modo::tracing::init(&config.modo.tracing);

    let (reader, writer) = db::connect_rw(&config.modo.database).await?;
    db::migrate("./migrations", &writer).await?;

    let mut registry = service::Registry::new();
    registry.add(reader.clone());
    registry.add(writer.clone());
    registry.add(modo::session::Store::new(&config.modo.session, writer.clone()));
    registry.add(modo::email::Mailer::new(&config.modo.email));
    registry.add(modo::upload::Storage::new(&config.modo.upload));
    registry.add(modo::job::Enqueuer::new(writer.clone()));
    registry.add(modo::template::Engine::builder()
        .templates("templates/")
        .static_files("static/", "/assets")
        .i18n("locales/")
        .build());

    let worker = modo::job::Worker::new(&config.modo.job, &registry)
        .register("send_welcome_email", send_welcome_email)
        .start()
        .await;

    let scheduler = modo::cron::Scheduler::new(&registry)
        .job("@every 15m", cleanup_sessions)
        .start()
        .await;

    let router = Router::new()
        .nest("/api/todos", todo::routes())
        .nest("/api/users", user::routes())
        .layer(modo::middleware::cors(&config.modo.cors, cors::urls(&config.modo.cors.origins)))
        .layer(modo::middleware::csrf(&config.modo.csrf))
        .layer(modo::middleware::rate_limit(&config.modo.rate_limit, rate_limit::by_ip()))
        .layer(modo::middleware::request_id())
        .layer(modo::session::layer(&registry))
        .with_state(registry.into_state());

    let server = modo::server::http(router, &config.modo.server).await;

    modo::runtime::run(vec![
        server,
        worker,
        scheduler,
        db::managed(writer),
        db::managed(reader),
    ]).await
}
```

## Extractors & Request Handling

Handlers are plain axum async functions. modo provides extractors and a result type.

### Request Body Extractors (one per handler)

| Extractor | Content-Type | Does |
|---|---|---|
| `JsonRequest<T>` | `application/json` | Deserialize + sanitize |
| `FormRequest<T>` | `application/x-www-form-urlencoded` | Deserialize + sanitize |
| `MultipartRequest<T>` | `multipart/form-data` | Deserialize text fields + sanitize + file access |

### Non-Body Extractors (any number per handler)

| Extractor | Purpose |
|---|---|
| `Service<T>` | Read from service registry |
| `Path<T>` | Path parameters |
| `Query<T>` | Query string (+ sanitize) |
| `Session` | Current session |
| `Tenant<T>` | Resolved tenant |
| `Option<Auth>` / `Auth` | Via middleware, not extractor |

### Response Types

`Json<T>`, `Html<String>`, `Redirect`, `Response` (axum's `Response` for mixed return types).

### Error Handling

```rust
pub type Result<T> = std::result::Result<T, Error>;

pub struct Error {
    status: StatusCode,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}
```

Built-in `From` conversions: `sqlx::Error` → 500 (unique → 409, not found → 404), `ValidationError` → 422, `lettre::Error` → 500, `std::io::Error` → 500. User adds their own via `impl From<MyError> for modo::Error`.

Panics are caught by `CatchPanicLayer` (included in the middleware stack) and converted to 500 errors before reaching the global error handler.

Global error handler middleware — one function handles all errors:

```rust
let router = Router::new()
    .layer(modo::middleware::error_handler(my_error_handler));

async fn my_error_handler(err: modo::Error, req: &Request) -> Response {
    if is_json(accept) { /* JSON error */ }
    else if is_htmx(req) { /* toast/alert partial */ }
    else { /* error page */ }
}
```

Handlers use `?` freely. Validation errors in forms are handled inline (need form values for re-render). Everything else propagates to the global handler.

### Validate & Sanitize

Builder API, no macros:

```rust
impl Validate for CreateTodo {
    fn validate(&self) -> Result<(), ValidationError> {
        Validator::new()
            .field("title", &self.title).required().min_length(3).max_length(100)
            .field("email", &self.email).required().email()
            .check()
    }
}

impl Sanitize for CreateTodo {
    fn sanitize(&mut self) {
        sanitize::trim(&mut self.title);
        sanitize::normalize_email(&mut self.email);
    }
}
```

Extractors run `sanitize()` automatically if `T: Sanitize`. User calls `body.validate()?` explicitly in the handler.

**Validation rules:** `required`, `min_length`, `max_length`, `email`, `url`, `range`, `one_of`, `matches_regex`, `custom(fn)`.

**Sanitizer functions:** `trim`, `trim_lowercase`, `collapse_whitespace`, `strip_html`, `truncate`, `normalize_email` (lowercase + strip plus-addressing).

## Database

Pure sqlx. No ORM. Compile-time backend choice via feature flag.

### Connection Modes

```rust
// Simple — one pool
let db = modo::db::connect(&config.database).await?;

// Read/write split — separate pools with per-pool PRAGMA config
let (reader, writer) = modo::db::connect_rw(&config.database).await?;
```

`Pool`, `ReadPool`, `WritePool` — newtype wrappers. `ReadPool` cannot be passed to migration functions (compile-time enforcement via `AsPool` trait).

### Config

```yaml
database:
  path: data/app.db
  max_connections: 10
  journal_mode: WAL
  synchronous: NORMAL
  busy_timeout: 5000
  reader:
    max_connections: 8
    busy_timeout: 1000
    cache_size: -16000
    mmap_size: 268435456
  writer:
    max_connections: 1
    busy_timeout: 2000
```

Per-pool overrides for pool sizing, timing, and PRAGMA values. This is the SQLite config; Postgres config uses a different structure:

```yaml
# Postgres config (when feature = "postgres")
database:
  url: ${DATABASE_URL}
  max_connections: 10
  min_connections: 1
  acquire_timeout_secs: 30
  idle_timeout_secs: 600
  max_lifetime_secs: 1800
  reader:
    url: ${DATABASE_READER_URL}     # read replica
    max_connections: 8
  writer:
    max_connections: 2
```

Config structs are feature-gated — `modo::db::Config` resolves to `SqliteConfig` or `PostgresConfig` depending on the enabled feature.

### Migrations

Two options — modo doesn't own migrations:

```rust
// Runtime — reads from disk
modo::db::migrate("./migrations/main", &writer).await?;

// Compile-time — sqlx's macro, embedded in binary
sqlx::migrate!("migrations/main").run(&writer).await?;
```

### Helpers

- `db::connect()` / `connect_rw()` — config-driven pool with PRAGMAs
- `db::migrate()` — run migrations from disk
- `db::new_id()` — ULID string generation
- `db::managed(pool)` — wrap pool as a `Task` for runtime shutdown
- Error conversion: `sqlx::Error` → `modo::Error` (not found → 404, unique violation → 409, etc.)

## Sessions

DB-backed sessions. SHA-256 hashed tokens. Fingerprinting. Token rotation. LRU eviction.

### Security Model

- **Token hashing** — raw 32-byte token in cookie, SHA-256 hash in DB. Compromised DB can't replay.
- **Fingerprinting** — SHA-256 of `User-Agent + Accept-Language + Accept-Encoding`. Mismatch → session destroyed.
- **Session fixation prevention** — `authenticate()` rotates token and sets `user_id` on existing session (preserves data like cart, favorites).
- **LRU eviction** — atomic within creation transaction. Over `max_sessions_per_user` → oldest evicted.
- **Stale cookie cleanup** — cookie exists but no session in DB → auto-removes cookie.
- **DB error isolation** — DB failure during read → treat as unauthenticated, don't delete cookie.
- **Token redaction** — `Debug`/`Display` emit `****`, `token_hash` skipped on serialization.

### Config

```yaml
session:
  anonymous: false               # true = create session before auth (shopping cart, etc.)
  cookie_name: _session
  session_ttl_secs: 2592000      # 30 days
  touch_interval_secs: 300       # 5 min between expiry refreshes
  validate_fingerprint: true
  max_sessions_per_user: 10
  trusted_proxies: []
  cookie:
    secure: true
    http_only: true
    same_site: lax
    path: /
```

### Handler API

```rust
async fn dashboard(session: Session) -> Result<Response> {
    // Read
    session.user_id()                          // Option<String>
    session.get::<T>("key")?                   // Option<T>
    session.is_authenticated()                 // bool
    session.current()                          // Option<SessionData>

    // Write (in-memory, flushed on response)
    session.set("key", &value)?
    session.remove_key("key")?

    // Auth lifecycle
    session.authenticate("user-id")?           // set user_id + rotate token
    session.authenticate_with("user-id", json!({...}))?  // with initial data

    // Token rotation
    session.rotate()?                          // new token, same session

    // Logout
    session.logout()?                          // destroy current
    session.logout_all()?                      // destroy all for user
    session.logout_other()?                    // destroy all except current

    // Session management
    session.list_my_sessions()?                // all active for user
    session.revoke(&other_session_id)?         // destroy specific (same user only)
}
```

### Middleware Lifecycle

1. **Request in** → read cookie → load session from DB → validate fingerprint → inject state
2. **Handler runs** → all reads/writes in-memory only
3. **Response out** → if data changed OR touch interval passed → single DB write. Apply cookie action.

Auth/logout do DB writes inline (create/destroy rows, rotate tokens).

### Session Internals

`Session` is a `FromRequestParts` extractor that holds `Arc<SessionState>`. `SessionState` contains:
- `store: session::Store` (holds a DB pool clone)
- `current: Mutex<Option<SessionData>>` — in-memory session data
- `dirty: AtomicBool` — tracks whether data was modified
- `action: Mutex<SessionAction>` — pending cookie action (Set/Remove/None)

Created per-request by the session middleware and injected into request extensions. The middleware's post-handler step reads `dirty` and `action` to decide what to flush.

### Session Cleanup

```rust
// As a cron job
cron.job("@every 15m", cleanup_sessions)

// Or manually
modo::session::cleanup_expired(&db).await?;
```

### DB Table (user-owned migration)

```sql
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    ip_address TEXT NOT NULL,
    user_agent TEXT NOT NULL,
    device_name TEXT NOT NULL,
    device_type TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    data TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    last_active_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
CREATE INDEX idx_sessions_user_id ON sessions(user_id);
CREATE INDEX idx_sessions_expires_at ON sessions(expires_at);
```

## Auth

Guards + password utilities. No user management, no user struct.

### Middleware Guards

```rust
// Protected routes — 401 / redirect if not authenticated
.layer(modo::auth::required("/login"))        // redirect for web
.layer(modo::auth::required_json())           // 401 for API

// Guest-only routes — 403 / redirect if authenticated
.layer(modo::auth::guest_only("/dashboard"))
.layer(modo::auth::guest_only_json())
```

### Password Hashing (Argon2id)

```rust
let hash = modo::auth::hash_password("password")?;
modo::auth::verify_password("password", &hash)?;
```

### TOTP (2FA)

```rust
let secret = modo::auth::totp::generate_secret();
let qr_url = modo::auth::totp::qr_url(&secret, "user@email.com", "MyApp");
modo::auth::totp::verify(&secret, "123456")?;
```

### Backup Codes

```rust
let codes = modo::auth::backup_codes::generate(10);
modo::auth::backup_codes::verify("ABCD-EFGH", &stored_hashes)?;
```

### OTP (Email/SMS codes)

```rust
let otp = modo::auth::otp::generate(6);  // { code, hash, expires_at }
// Store hash + expires_at, send code via email/SMS
modo::auth::otp::verify("123456", &stored_hash, expires_at)?;
```

### Password Generation

```rust
let password = modo::auth::password::generate(16);
let passphrase = modo::auth::password::passphrase(4);  // "correct-horse-battery-staple"
```

All stateless functions. User owns DB schema, login handlers, user management.

## OAuth (feature-gated)

```rust
let google = modo::oauth::Google::new(&config.oauth.google);
let github = modo::oauth::Github::new(&config.oauth.github);
registry.add(google);
registry.add(github);
```

```rust
// Redirect to provider
async fn google_login(Service(google): Service<GoogleOAuth>) -> Result<Response> {
    let (url, state) = google.authorize_url();
    session.set("oauth_state", &state)?;
    Ok(Redirect::to(&url).into_response())
}

// Callback
async fn google_callback(
    session: Session,
    Service(google): Service<GoogleOAuth>,
    Query(params): Query<OAuthCallback>,
) -> Result<Response> {
    let saved_state = session.get::<String>("oauth_state")?;
    let oauth_user = google.exchange(&params.code, saved_state.as_deref()).await?;
    // oauth_user: { provider_id, email, name, avatar_url }
    // User's responsibility: find-or-create user, link account
    session.authenticate(&user.id)?;
    Ok(Redirect::to("/dashboard").into_response())
}
```

Config:

```yaml
oauth:
  google:
    client_id: ...
    client_secret: ...
    redirect_url: https://myapp.com/auth/google/callback
  github:
    client_id: ...
    client_secret: ...
    redirect_url: https://myapp.com/auth/github/callback
```

## Multi-tenancy

Tenant resolution from HTTP request with custom resolve function.

### Resolvers

```rust
modo::tenant::subdomain(&config.tenant)
modo::tenant::header("X-Tenant-Id")
modo::tenant::path_prefix("/t")
```

### Resolve Function (DB validation + data loading)

```rust
let resolver = modo::tenant::subdomain(&config.tenant)
    .resolve(|slug: &str, registry: &service::Registry| async move {
        let db = registry.get::<ReadPool>();
        sqlx::query_as!(Org,
            "SELECT id, name, plan FROM organizations WHERE slug = ?", slug
        ).fetch_optional(db).await?
        .ok_or(modo::Error::not_found("Tenant not found"))
    });
```

### Extractor

```rust
// Required — 400 if no tenant resolved
async fn dashboard(tenant: Tenant<Org>) -> Result<Response> {
    let org = tenant.get();  // &Org
}

// Optional
async fn landing(tenant: Option<Tenant<Org>>) -> Result<Response> { ... }
```

`Tenant<T>` is generic over whatever the resolve function returns. Without `.resolve()`, `Tenant<String>` holds the raw extracted value.

## Templates, i18n, Static Files

### Setup

```rust
let engine = modo::template::Engine::builder()
    .templates("templates/")
    .static_files("static/", "/assets")
    .i18n("locales/")
    .build();
```

Debug mode: reads from disk (live reload). Release mode: embedded in binary via `include_dir!`.

### Rendering

```rust
async fn dashboard(
    Service(engine): Service<modo::template::Engine>,
) -> Result<Response> {
    Ok(engine.render("dashboard.html", context! {
        todos: todos,
    })?.into_response())
}
```

### Predefined Template Context

```rust
let engine = modo::template::Engine::builder()
    .with_default_context()  // locale, csrf_token, current_url, is_htmx
    .with_context(|req| context! {
        app_name: "MyApp",
        user_id: req.session().user_id(),
    })
    // ...
```

Auto-injected into every render. Handler context merges on top.

### HTMX Support

```rust
async fn todo_list(
    hx: modo::template::HxRequest,
    Service(engine): Service<modo::template::Engine>,
) -> Result<Response> {
    let template = if hx.is_htmx() { "todos/_list.html" } else { "todos/list.html" };
    Ok(engine.render(template, context! { todos })?.into_response())
}
```

### i18n

Translation files in `locales/`:

```yaml
# en.yaml
hello:
  greeting: "Hello, {{ name }}!"
```

In templates: `{{ t("hello.greeting", name="Dmytro") }}`

**Locale resolution** (configurable priority chain):

1. Query param (`?lang=uk`)
2. Cookie
3. Session
4. Accept-Language header (with weight parsing)
5. Default locale

### Static Files

| | Dev | Release |
|---|---|---|
| Source | Disk (every request) | Embedded in binary |
| `static_url()` version | Unix timestamp | Content hash |
| Cache-Control | `no-cache` | `max-age=31536000, immutable` |
| ETag | None | SHA-256 content hash |
| 304 support | No | Yes |

In templates: `{{ static_url('css/app.css') }}` → `/assets/css/app.css?v=a3f2b1c4`

No directory listing. Path traversal prevention. Content-Type from file extension.

## SSE (Server-Sent Events)

### Broadcast Manager

```rust
let sse = modo::sse::Broadcast::<ChatMessage>::new(128);
registry.add(sse);
```

### API

```rust
sse.stream(channel) -> SseStream<T>      // subscribe
sse.send(channel, event)                  // publish
sse.subscriber_count(channel) -> usize
sse.remove(channel)                       // force-disconnect all
```

Channels are lazy (created on first subscribe, auto-removed on last unsubscribe).

### Stream Transforms

```rust
.as_json()                      // T: Serialize → JSON event
.as_html(|&T| -> String)       // render → HTML event
.as_event(|&T| -> SseEvent)    // full control
```

### EventId Trait

```rust
pub trait EventId {
    fn event_id(&self) -> Option<&str>;
}
```

If `T: EventId`, stream transforms auto-set the SSE `id:` field.

### LastEventId

```rust
async fn events(
    Service(sse): Service<Broadcast<TicketMessage>>,
    last_event_id: LastEventId,
) -> Result<impl IntoResponse> {
    let replay = if let Some(last_id) = last_event_id.value() {
        fetch_events_after(last_id, &*db).await?
    } else {
        vec![]
    };
    Ok(sse.stream("ticket:123").with_replay(replay).as_json())
}
```

### SseEvent Builder

```rust
SseEvent::new()
    .event("notification")
    .json(&data)?
    .html("<div>update</div>")
    .id("evt-123")
    .retry(Duration::from_secs(5))
```

### Imperative Channel (for custom producers)

```rust
async fn dashboard_stream() -> impl IntoResponse {
    modo::sse::channel(|tx| async move {
        loop {
            let stats = get_stats().await?;
            tx.send(SseEvent::new().json(&stats)?).await?;
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
}
```

Response features: configurable keep-alive interval, `X-Accel-Buffering: no` header.

## Background Jobs

### Enqueuer (standalone, just inserts rows)

```rust
let enqueuer = modo::job::Enqueuer::new(writer.clone());
registry.add(enqueuer);
```

```rust
// In handlers
async fn register_user(
    Service(jobs): Service<modo::job::Enqueuer>,
) -> Result<Json<User>> {
    jobs.enqueue("send_welcome_email", &WelcomePayload { user_id: user.id }).await?;
    jobs.enqueue_at("generate_report", &payload, run_at).await?;
    jobs.cancel(&job_id).await?;

    // Deduplicated — skip if same name+payload exists within TTL
    jobs.enqueue_unique("send_report", &payload, Duration::from_secs(300)).await?;
}
```

`enqueue_unique()` computes SHA-256 of `name + payload`, checks DB for existing pending/running job with same hash within TTL. Returns `EnqueueResult::Created(id)` or `EnqueueResult::Duplicate(id)`.

### Worker (polls, claims, executes)

```rust
let worker = modo::job::Worker::new(&config.modo.job, &registry)
    .register("send_welcome_email", send_welcome_email)
    .register_with("process_payment", process_payment, modo::job::Options {
        queue: "email",
        priority: 10,
        max_attempts: 5,
        timeout: Duration::from_secs(300),
    })
    .start()
    .await;
```

### Job Handlers (extractors, same as HTTP)

```rust
async fn send_welcome_email(
    payload: modo::job::Payload<WelcomePayload>,
    Service(mailer): Service<modo::email::Mailer>,
    Service(db): Service<ReadPool>,
) -> Result<()> {
    let user = sqlx::query_as!(User, "SELECT * FROM users WHERE id = ?", payload.user_id)
        .fetch_one(&*db).await?;
    mailer.send(modo::email::SendEmail::new("welcome", &user.email)
        .var("name", &user.name)).await
}
```

Available extractors: `Payload<T>`, `Service<T>`, `modo::job::Meta` (id, name, queue, attempt).

### Job Lifecycle

```
enqueue → Pending
            ↓ (atomic claim: UPDATE ... RETURNING)
          Running
            ├── success → Completed
            ├── failure (attempts < max) → Pending (exponential backoff)
            ├── failure (attempts >= max) → Dead
            ├── timeout → failure path
            └── panic → failure path (catch_unwind)
cancel → Cancelled (from Pending only)
stale reaper → Running back to Pending (attempts decremented)
cleanup → deletes terminal states after retention period
```

Backoff: `min(5 * 2^(attempt-1), 3600)` seconds.

### Config

```yaml
job:
  poll_interval_secs: 1
  stale_threshold_secs: 600
  stale_reaper_interval_secs: 60
  drain_timeout_secs: 30
  max_payload_bytes: null
  max_queue_depth: null
  queues:
    - name: default
      concurrency: 4
    - name: email
      concurrency: 2
  cleanup:
    interval_secs: 3600
    retention_secs: 86400
    statuses: [completed, dead, cancelled]
```

### Unregistered Jobs

Worker only claims jobs matching registered handler names. Unregistered jobs stay `Pending` until they expire or a worker that knows the handler picks them up.

### Sidecar Worker

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::load::<AppConfig>("config/")?;
    let (reader, writer) = db::connect_rw(&config.modo.database).await?;

    let mut registry = service::Registry::new();
    registry.add(reader.clone());
    registry.add(writer.clone());
    registry.add(modo::email::Mailer::new(&config.modo.email));

    let worker = modo::job::Worker::new(&config.modo.job, &registry)
        .register("send_welcome_email", send_welcome_email)
        .start().await;

    modo::runtime::run(vec![
        worker,
        db::managed(writer),
        db::managed(reader),
    ]).await
}
```

## Cron

In-memory recurring task scheduler. Separate from job queue — no DB.

```rust
let scheduler = modo::cron::Scheduler::new(&registry)
    .job("@every 15m", cleanup_sessions)
    .job("@daily", daily_digest)
    .job_with("@every 30s", health_check, modo::cron::Options {
        timeout: Duration::from_secs(10),
    })
    .start()
    .await;
```

### Fluent Schedules

| Alias | Description |
|---|---|
| `@yearly` / `@annually` | Midnight Jan 1 |
| `@monthly` | Midnight 1st of month |
| `@weekly` | Midnight Sunday |
| `@daily` / `@midnight` | Midnight |
| `@hourly` | Top of every hour |
| `@every <duration>` | Fixed interval (`1h`, `30m`, `15s`) |
| Standard cron | `0 0 9 * * MON-FRI` |

Validated at startup — invalid schedule panics. Sequential execution — if a run exceeds the interval, next tick is skipped.

### Cron Handlers (same extractors as jobs)

```rust
async fn cleanup_sessions(
    Service(db): Service<WritePool>,
) -> Result<()> {
    sqlx::query!("DELETE FROM sessions WHERE expires_at < ?", Utc::now())
        .execute(&*db).await?;
    Ok(())
}
```

## Email

SMTP only. Markdown templates with YAML frontmatter. Layout engine. LRU caching.

### Setup

```rust
let mailer = modo::email::Mailer::new(&config.email);
registry.add(mailer);
```

### Config

```yaml
email:
  templates_path: emails
  default_from_name: MyApp
  default_from_email: noreply@myapp.com
  default_reply_to: support@myapp.com
  cache_templates: true
  template_cache_size: 100
  smtp:
    host: smtp.mailgun.com
    port: 587
    username: postmaster@myapp.com
    password: ${SMTP_PASSWORD}
    security: starttls
```

### Templates

Markdown with YAML frontmatter in `emails/{locale}/{name}.md`:

```markdown
---
subject: "Welcome to {{product_name}}, {{name}}!"
layout: default
---

Hi **{{name}}**,

Thanks for signing up!

[button|Get Started]({{dashboard_url}})
```

Features:
- `{{var}}` substitution (HTML-escaped in body, raw in subject)
- Markdown → HTML + plain text in single pass
- `[button|Label](url)` — email-safe table-based buttons (Outlook compatible)
- Brand color via `brand_color` context variable
- Locale fallback (`uk/welcome.md` → `en/welcome.md`)
- LRU cache keyed by `(name, locale)`
- Built-in responsive layout with dark mode, mobile support, optional logo/footer
- Custom layouts in `layouts/*.html`
- Templates embedded in binary (release), disk (dev with hot-reload)

### Sending

```rust
mailer.send(
    modo::email::SendEmail::new("welcome", "user@example.com")
        .locale("uk")
        .var("name", "Dmytro")
        .var("product_name", "MyApp")
        .to("another@example.com")            // additional recipient
        .sender(modo::email::SenderProfile {   // optional override
            from_name: "Team".into(),
            from_email: "team@myapp.com".into(),
            reply_to: Some("team@myapp.com".into()),
        })
).await?;
```

### Render without sending

```rust
let message = mailer.render(&email)?;
// message.html, message.text, message.subject
```

## Uploads

S3-compatible storage. Multipart extraction. Presigned URLs.

### Setup

```rust
let storage = modo::upload::Storage::new(&config.upload);
registry.add(storage);
```

### Config

```yaml
upload:
  bucket: my-app-uploads
  region: us-east-1
  endpoint: https://s3.amazonaws.com
  access_key: xxx
  secret_key: xxx
  max_file_size: 10485760
  allowed_types: [image/jpeg, image/png, image/webp, application/pdf]
```

### Storage Methods

```rust
let path = storage.put(&file, "avatars/").await?;                    // returns S3 key
let path = storage.put_with(&file, "docs/", PutOptions {
    acl: Acl::Private,
    content_disposition: Some("attachment".into()),
    cache_control: Some("max-age=31536000".into()),
}).await?;
storage.delete("avatars/photo.jpg").await?;
storage.delete_prefix("avatars/user-123/").await?;                   // all under prefix
let url = storage.url("avatars/photo.jpg");                          // no network call
let url = storage.presigned_url("docs/report.pdf", Duration::from_secs(3600));  // no network call
```

### ACL

```rust
pub enum Acl {
    Private,
    PublicRead,
    PublicReadWrite,
}
```

### UploadedFile

```rust
pub struct UploadedFile {
    pub name: String,
    pub content_type: String,
    pub size: usize,
    pub data: Bytes,
}
```

### Network Calls

| Method | Hits S3? |
|---|---|
| `put()` / `put_with()` | Yes |
| `delete()` / `delete_prefix()` | Yes |
| `url()` | No |
| `presigned_url()` | No |

## Middleware

Each returns a `tower::Layer`. User controls order.

### Available Middleware

```rust
modo::middleware::request_id()                           // ULID per request
modo::middleware::tracing()                              // structured request logging
modo::middleware::security_headers(&config.security)     // configurable security headers
modo::middleware::compression()                          // gzip/brotli/zstd
modo::middleware::cors(&config.cors, origin_strategy)    // CORS
modo::middleware::csrf(&config.csrf)                     // double-submit cookie
modo::middleware::rate_limit(&config.rate_limit, key_fn) // token bucket
modo::session::layer(&registry)                          // session middleware
modo::middleware::error_handler(handler_fn)              // global error handler
```

### CORS Origin Strategies

```rust
// Static URLs from config
modo::middleware::cors(&config.cors, cors::urls(&config.cors.origins))

// All subdomains
modo::middleware::cors(&config.cors, cors::subdomains("myapp.com"))

// Custom function (multi-tenant custom domains)
modo::middleware::cors(&config.cors, |origin: &str, db: &Pool| async move {
    sqlx::query_scalar!("SELECT EXISTS(SELECT 1 FROM tenants WHERE custom_domain = ?)", origin)
        .fetch_one(db).await.unwrap_or(false)
})
```

Static origins from config are checked first. If no match, the function is called.

### Rate Limit Key Functions

```rust
modo::middleware::rate_limit(&config.rate_limit, rate_limit::by_ip())
modo::middleware::rate_limit(&config.rate_limit, rate_limit::by_header("X-Api-Key"))
modo::middleware::rate_limit(&config.rate_limit, rate_limit::by_user())
modo::middleware::rate_limit(&config.rate_limit, |req: &RequestParts| {
    format!("{}:{}", req.client_ip(), req.uri().path())
})
```

## Config

YAML files with environment variable substitution.

### Loading

```rust
let config = modo::config::load::<AppConfig>("config/")?;
```

Reads `APP_ENV` env var (default: `development`), loads `config/{APP_ENV}.yaml`. No merging, no cascading.

### Env Var Substitution (opt-in)

```yaml
email:
  smtp:
    host: ${SMTP_HOST}
    password: ${SMTP_PASSWORD}
env: ${APP_ENV:development}
```

`${VAR}` — required. `${VAR:default}` — with fallback.

### User Config

```rust
#[derive(Deserialize)]
struct AppConfig {
    #[serde(flatten)]
    pub modo: modo::Config,
    pub stripe_key: String,
    pub app_name: String,
}
```

All modo config sections implement `Deserialize + Default`. Missing sections get struct defaults.

### Environment Helpers

```rust
modo::config::env()        // reads APP_ENV, defaults to "development"
modo::config::is_dev()
modo::config::is_prod()
modo::config::is_test()
```

## Test Helpers

### Full App Test

```rust
#[tokio::test]
async fn test_create_todo() {
    let app = modo::test::app("config/")
        .fixtures(&["users", "todos"])
        .build()
        .await;

    let res = app.post("/api/todos")
        .json(&json!({"title": "Buy milk"}))
        .session("user_id", "user-123")
        .send()
        .await;

    assert_eq!(res.status(), 201);
    assert_eq!(res.json::<Todo>().await.title, "Buy milk");
}
```

`modo::test::app()` sets `APP_ENV=test` automatically.

### Test Client

```rust
app.get("/path").send().await
app.post("/path").json(&body).send().await
app.post("/path").form(&body).send().await
app.put("/path").json(&body).send().await
app.delete("/path").send().await
app.get("/path").header("Authorization", "Bearer token").send().await
app.get("/dashboard").session("user_id", "user-123").send().await
```

### Test Response

```rust
res.status()
res.json::<T>().await
res.text().await
res.html().await
res.header("X-Request-Id")
res.cookie("_session")
```

### Function-Level Tests

```rust
#[tokio::test]
async fn test_create_todo_logic() {
    let db = modo::test::db("config/").await;
    let result = create_todo(
        Service(db.writer()),
        JsonRequest(CreateTodo { title: "Buy milk".into() }),
    ).await;
    assert!(result.is_ok());
}
```

### Fixtures

SQL files in `tests/fixtures/{name}.sql`, executed after migrations:

```rust
let app = modo::test::app("config/")
    .fixtures(&["users", "todos"])
    .build().await;
```

## Tracing & Sentry

### Config

```yaml
tracing:
  level: info
  format: pretty               # pretty (dev) | json (production)
  sentry:
    dsn: ${SENTRY_DSN:}        # empty = disabled
    environment: ${APP_ENV:development}
    sample_rate: 1.0
    traces_sample_rate: 0.1
```

### Setup

```rust
modo::tracing::init(&config.modo.tracing);
```

- Sentry DSN empty → stdout only
- Sentry DSN set → stdout + Sentry (both receive events)

Automatic: `tracing::error!` → Sentry events, panics → crash reports, request spans → performance transactions, `request_id` + `user_id` attached as context.

## Runtime Orchestrator

Manages application lifecycle: waits for shutdown signal, stops tasks sequentially.

### Task Trait

```rust
pub trait Task: Send + 'static {
    fn shutdown(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>;
}
```

Object-safe — uses `self: Box<Self>` and returns a boxed future so different task types can coexist in a `Vec<Box<dyn Task>>`. A blanket helper converts async functions:

```rust
// Convenience: implement with a simple async block
impl Task for ManagedPool {
    fn shutdown(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        Box::pin(async move {
            self.pool.close().await;
            Ok(())
        })
    }
}
```

### Usage

All tasks are already running when passed to `runtime::run()`. On SIGTERM/SIGINT, each task's `shutdown()` is called sequentially in order:

```rust
modo::runtime::run(vec![
    Box::new(server),              // stops first: drain HTTP connections
    Box::new(worker),              // stops second: finish in-flight jobs
    Box::new(scheduler),           // stops third: cancel pending cron
    db::managed(writer),           // stops fourth: close write pool (returns Box<dyn Task>)
    db::managed(reader),           // stops last: close read pool
]).await
```

Global timeout — if any task hangs, log warning and exit.

## Implementation Notes

### Service Registry

`service::Registry` is a `HashMap<TypeId, Arc<dyn Any + Send + Sync>>`. Each `.add(value)` inserts by `TypeId::of::<T>()`. `Pool`, `ReadPool`, `WritePool` are distinct newtypes with distinct `TypeId`s — no collision. `registry.into_state()` wraps the registry in an `Arc` and returns an axum-compatible `AppState`. The `Service<T>` extractor reads from `State<AppState>` via `FromRequestParts`.

### Job/Cron Handler Extractors

Job and cron handlers use a separate `FromJobContext` trait (not axum's `FromRequestParts`). `modo::job::Payload<T>`, `Service<T>`, and `modo::job::Meta` implement `FromJobContext`. The worker builds a synthetic `JobContext` from the registry + payload JSON + job metadata, then resolves extractors against it. Same pattern as axum extractors, different trait.

### Database Transactions

sqlx's native transaction API works directly:

```rust
let txn = db.begin().await?;
sqlx::query!("INSERT INTO orders ...").execute(&mut *txn).await?;
sqlx::query!("UPDATE inventory ...").execute(&mut *txn).await?;
txn.commit().await?;
```

modo does not wrap or abstract transactions — use sqlx directly.

### Middleware Ordering

Tower middleware ordering: last `.layer()` call = outermost (runs first on request, last on response). Recommended order (outermost → innermost):

```rust
.layer(modo::middleware::error_handler(handler))     // catches all errors
.layer(modo::middleware::compression())               // compress responses
.layer(modo::middleware::security_headers(&config))   // add security headers
.layer(modo::middleware::cors(&config, strategy))     // handle CORS preflight
.layer(modo::middleware::request_id())                // generate request ID
.layer(modo::middleware::tracing())                   // log request/response
.layer(modo::middleware::rate_limit(&config, key_fn)) // rate limit
.layer(modo::middleware::csrf(&config))               // CSRF protection
.layer(modo::session::layer(&registry))               // session (innermost)
```

### Static File Serving

Static files are served via a nested axum service on the engine's configured prefix. When `template::Engine` is added to the registry and the session layer is applied, the engine auto-registers a static file route on the router. Alternatively, user can mount manually:

```rust
let router = Router::new()
    .nest_service("/assets", engine.static_service())
    // ...
```

### Cookie Key Management

Session cookies are signed using a `Key` derived from a configurable secret in the session config. If no secret is provided, a random key is generated at startup (sessions won't survive restarts). For production, users should set an explicit secret:

```yaml
session:
  cookie_secret: ${SESSION_SECRET}  # 64+ character hex string
```

### Test Session Helper

`app.post("/path").session("user_id", "user-123")` creates a real session row in the test DB, sets the cookie on the request. Full session middleware runs — test fidelity is high.

### Compile Time Tradeoffs

Collapsing all modules into one crate means changing any module triggers a full crate recompile. Mitigations: aggressive feature gating (unused features don't compile), Rust incremental compilation (only changed code recompiles within the crate), and the removal of heavy dependencies (SeaORM, proc macros) significantly reduces baseline compile time compared to v1.

### Presigned URLs

`storage.presigned_url()` computes the signature locally using the configured credentials and AWS Signature V4 algorithm. No network call. Works with any S3-compatible service that supports V4 signing.
