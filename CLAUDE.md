# modo

Rust web framework for micro-SaaS. Single binary, compile-time magic, multi-DB support.

## Stack

- axum 0.8 (HTTP)
- SeaORM v2 RC (database) — use v2 only, not v1.x
- MiniJinja (templates)
- inventory (auto-discovery, not linkme)
- tokio (async runtime)

## Architecture

- `modo/` — core crate (HTTP, cookies, services, templates, CSRF, i18n — no DB)
  - Feature `templates` — MiniJinja template engine (views, render layer, context injection)
  - Feature `csrf` — CSRF protection (double-submit cookie, HMAC-signed tokens)
  - Feature `i18n` — internationalization (YAML translations, locale middleware, `t!()` macro)
  - Feature `static-fs` — serve static files from filesystem
  - Feature `static-embed` — embed static files via `rust-embed`
- `modo-macros/` — core proc macros (`#[handler]`, `#[main]`, `#[module]`, `#[view]`, `t!()`, `Sanitize`, `Validate`)
- `modo-db/` — database layer (features: sqlite, postgres)
- `modo-db-macros/` — database proc macros
- `modo-session/` — session management
- `modo-auth/` — authentication
- `modo-jobs/` — background jobs
- `modo-jobs-macros/` — `#[job(...)]` proc macro
- `modo-upload/` — file uploads
- `modo-upload-macros/` — upload proc macros
- `modo-tenant/` — multi-tenancy (tenant resolution, extractors, template context)

## Commands

- `just fmt` — format all code
- `just lint` — clippy with `-D warnings` (all workspace targets/features)
- `just test` — run all workspace tests
- `just check` — fmt-check + lint + test (CI/pre-push)
- `cargo check` — type check
- `cargo build -p hello` — build example
- `cargo run -p hello` — run example server

## Conventions

- Cookie building: use `cookies::build_cookie()` + `CookieOptions::from_config(&cookie_config)` with overrides — never hand-format `Set-Cookie` strings
- `CookieConfig` is always auto-registered in `app.rs` services; middleware can read it via `state.services.get::<CookieConfig>()`
- Handlers: `#[modo::handler(METHOD, "/path")]`
- Path params: plain `id: String` in handler fn auto-extracted from `{id}` in route path — no need for `Path(id): Path<String>`
- Path params: partial extraction supported — declare only the params you need, others ignored via `..`
- Entry point: `#[modo::main]` — requires `(app: AppBuilder, config: MyConfig)` where config is auto-loaded via `load_or_default()`
- Routes auto-discovered via `inventory` crate
- DB extractor: `Db(db): Db`
- Service extractor: `Service<MyType>`
- Errors: `Result<T, Error>`
- Modules: `#[modo::module(prefix = "/path", middleware = [...])]`
- CSRF: `#[middleware(modo::middleware::csrf_protection)]` — uses double-submit cookie
- Flash messages: `Flash` (write) / `FlashMessages` (read) — cookie-based, one-shot
- Templates config: `TemplateConfig { path, strict }` — YAML-deserializable with serde defaults
- Template engine: `modo::templates::engine(&config)?` — config to engine (follows `modo::i18n::load` pattern)
- Views: `#[modo::view("pages/home.html")]` or `#[modo::view("page.html", htmx = "htmx/frag.html")]`
- View structs: fields must implement `Serialize`, handler returns struct directly
- Template context: `TemplateContext` in request extensions, middleware adds via `ctx.insert("key", value)`
- Template layers: auto-registered when `TemplateEngine` is a service — no manual `.layer()` needed
- HTMX views: htmx template rendered on HX-Request, always HTTP 200, non-200 skips render
- i18n in templates: `{{ t("key", name=val) }}` — register via `modo::i18n::register_template_functions`
- i18n layer: `modo::i18n::layer(store, Arc::new(cookie_config))` / `modo::i18n::layer_with_source(store, Arc::new(cookie_config), source_fn)`
- Middleware: plain async functions, attached via `#[middleware(fn_name(params))]`
- Middleware stacking order: Global (outermost) → Module → Handler (innermost)
- Services: manually constructed, registered via `.service(instance)`
- Sessions: `SessionStore::new(&db, session_config, cookie_config)` + `app.service(store.clone()).layer(modo_session::layer(store))`
- SessionManager extractor: `authenticate()` / `logout()` / `logout_all()` / `logout_other()` / `revoke(id)` / `rotate()` — handles cookies automatically
- SessionManager data: `get::<T>(key)` / `set(key, value)` / `remove_key(key)` — immediate store writes
- Auth: implement `UserProvider` trait, use `Auth<User>` / `OptionalAuth<User>` extractors
- Jobs: `#[modo_jobs::job(queue = "...", priority = N, max_attempts = N, timeout = "5m")]`
- Cron jobs: `#[modo_jobs::job(cron = "0 0 * * * *", timeout = "5m")]` — in-memory only
- Upload storage: `UploadConfig { backend, path, s3 }` — YAML-deserializable, `modo_upload::storage(&config)?` returns `Box<dyn FileStorage>`
- Tenant resolution: implement `HasTenantId` + `TenantResolver` traits, wrap in `TenantResolverService::new(resolver)`, register as service
- Tenant extractors: `Tenant<T>` (required, 404 if missing), `OptionalTenant<T>` (optional), both cache via `ResolvedTenant<T>` extension
- Built-in resolvers: `SubdomainResolver`, `HeaderResolver`, `PathPrefixResolver` — all take a lookup closure
- Template context layer: `TenantContextLayer<T>` — auto-injects `tenant` into `TemplateContext` (feature = "templates")
- User context layer: `modo_auth::context_layer::UserContextLayer<U>` — auto-injects `user` into `TemplateContext` (feature = "templates")

## Gotchas

- Feature flags: optional deps use `dep:name` syntax; gate fields with `#[cfg(feature = "...")]` in struct, Default, and from_env()
- Proc macros can't check `cfg` flags — emit both `#[cfg(feature = "x")]` / `#[cfg(not(feature = "x"))]` branches in generated code
- Always run `just fmt` before `just check` — format diffs fail the check early
- `-D warnings` means dead code is a build error — remove unused code, don't just make it `pub(crate)`
- Clippy enforces `collapsible_if` — collapse nested `if`/`if let` with `&&`
- Re-exports in `modo/src/lib.rs` must be alphabetically sorted (`cargo fmt` enforces this)
- `inventory` registration from library crates may not link in tests — force with `use crate::entity::foo as _;`
- SeaORM's `ExprTrait` conflicts with `Ord::max`/`Ord::min` — disambiguate with `Ord::max(a, b)` syntax
- Use official documentation only when researching dependencies
- Session IDs: ULID (no UUID anywhere)
- Testing Tower middleware: use `Router::new().route(...).layer(mw).oneshot(request)` pattern — no AppState needed, handler reads `Extension<T>` from extensions
- Testing cookie attributes: create `AppState` with custom `CookieConfig` (e.g. `domain`), fire request, assert `Set-Cookie` header contains expected attributes
- Type-erased services: use object-safe bridge trait (`XxxDyn`) + `Arc<dyn XxxDyn<T>>` wrapper — see `TenantResolverService` pattern
- Session user ID access: use `modo_session::user_id_from_extensions(&parts.extensions)` — returns `Option<String>`
