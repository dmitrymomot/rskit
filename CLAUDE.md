# modo v2

Clean rewrite of the modo Rust web framework. Single crate, no proc macros, plain functions, explicit wiring, raw sqlx.

## Branch Rules

- All work happens on branch `modo-v2` — no extra branches
- NEVER switch to or merge into `main` — main has v1 code
- Do NOT reference v1 patterns (SeaORM, inventory, proc macros, multi-crate workspace)

## Design Philosophy

- One crate (`modo`), zero proc macros
- Handlers are plain `async fn` — no macros, no signature rewriting
- Routes use axum's `Router` directly — no auto-registration
- Services wired explicitly in `main()` — no global discovery
- Database uses raw sqlx — no ORM
- Feature flags only for truly optional pieces (templates, SSE, auth, storage)
- No TODOs, no workarounds, no tech debt — every declared config field and API must be fully implemented

## Commands

- `cargo check` — type check
- `cargo test` — run all tests
- `cargo clippy -- -D warnings` — lint
- `cargo fmt --check` — format check
- `cargo fmt` — format code

## Workflow

- Use `superpowers:brainstorming` skill to design specs before implementation
- Use `superpowers:subagent-driven-development` skill for plan implementation

## Conventions

- Paths: NEVER use absolute paths — always relative to project root
- File organization: `mod.rs` and `lib.rs` are ONLY for `mod` imports and re-exports — all code goes in separate files
- Extractors: `Service<T>` reads from registry, `JsonRequest<T>` / `FormRequest<T>` for request bodies (require `T: Sanitize`), `Path<T>` / `Query<T>` for params
- Error handling: `modo::Error` with status + message + optional source; `modo::Result<T>` alias; `?` everywhere
- Error constructors: `Error::not_found()`, `Error::bad_request()`, `Error::internal()`, etc.
- Response types: `Json<T>`, `Html<String>`, `Redirect`, `Response`
- Service registry: `Registry` is `HashMap<TypeId, Arc<dyn Any + Send + Sync>>` — `.add(value)` inserts, `Service<T>` extracts
- Config: YAML with `${VAR}` / `${VAR:default}` env var substitution, loaded per `APP_ENV`
- Database: `Pool`, `ReadPool`, `WritePool` newtypes; `Reader`/`Writer` traits; `connect()` / `connect_rw()` for pools
- Database: `connect()` forces `max_connections=1` for `:memory:` — `connect_rw()` rejects `:memory:` entirely; for in-memory tests use one `Pool` and wrap via `ReadPool::new()`/`WritePool::new()` to share the same underlying connection
- IDs: `id::ulid()` for full ULID (26 chars), `id::short()` for short time-sortable ID (13 chars, base36) — no UUID
- Runtime: `Task` trait + `run!` macro for sequential shutdown
- Tracing fields: always snake_case (`user_id`, `session_id`, `job_id`)
- Pluggable backends: wrap with `Arc<dyn Trait>` (not `Box`)
- Cache: `src/cache/` module provides `LruCache` — always available, no feature gate
- Encoding: `src/encoding/` module provides `base32`, `base64url` encode/decode, and `hex` encode + `sha256` helper — always available, no feature gate
- Rate limiting: custom `KeyExtractor` trait in `src/middleware/rate_limit.rs` — `PeerIpKeyExtractor` for IP-based, `GlobalKeyExtractor` for shared bucket; `rate_limit()` and `rate_limit_with()` accept `CancellationToken` for cleanup shutdown
- Config: `trusted_proxies` is a top-level config field (not under `session`) — parsed into `Vec<IpNet>` at startup for `ClientIpLayer`

## Current Work

- **Plan 12 (Test Helpers):** DONE — `src/testing/` module behind `test-helpers` feature flag
- Test migration fixtures live at `tests/fixtures/migrations/` — used by `TestDb::migrate()` tests
- MaxMind test DB at `tests/fixtures/GeoIP2-City-Test.mmdb` — used by geolocation tests
- **Plan 13 (RBAC):** DONE — `src/rbac/` module with `RoleExtractor` trait, `Role` extractor, RBAC middleware, `require_role()` / `require_authenticated()` guard layers (22 unit + 8 integration tests)
- **Plan 14 (JWT):** DONE — `src/auth/jwt/` module with `JwtEncoder`/`JwtDecoder`, `Claims<T>`, `HmacSigner` (HS256), `JwtLayer<T>` middleware, pluggable `TokenSource`, optional `Revocation` trait, `Bearer` extractor. Feature-gated under `auth` (73 unit + 13 integration tests)
- **Plan 15 (Webhook Delivery):** DONE — `src/webhook/` module with `WebhookSender<C>`, `HttpClient` trait, `HyperClient`, `WebhookSecret`, Standard Webhooks signing. Feature-gated under `webhooks`
- **Plan 16 (Flash Messages):** DONE — `src/flash/` module with `Flash` extractor (`flash.success()` / `flash.set()` / `flash.messages()`), `FlashLayer` middleware, `flash_messages()` template function. Cookie-based (signed), read-once-and-clear. No session dependency. Always-available (no feature gate)
- **Plan 17 (Storage ACL + Upload from URL):** DONE — `src/storage/` extended with `Acl` enum on `PutOptions`, `x-amz-acl` S3 header, `PutFromUrlInput`, `put_from_url()` / `put_from_url_with()` with streaming fetch and 30s timeout. Feature-gated under `storage`
- **Plan 18 (DNS Verification):** DONE — `src/dns/` module with `DomainVerifier` (`check_txt()`, `check_cname()`, `verify_domain()`), `DnsConfig`, `DnsError`, `DomainStatus`, `generate_verification_token()`. Uses `simple-dns` 0.11 for packet parsing, raw UDP transport. Feature-gated under `dns` (39 unit + 5 integration tests)
- **Plan 19 (Client IP + Geolocation):** DONE — `src/ip/` shared module (always available) with `ClientIp` extractor + `ClientIpLayer` middleware; `src/geolocation/` with `GeoLocator` service, `Location` struct, `GeoLayer` middleware. Session refactored to use shared `ClientIp`. Feature-gated under `geolocation` (28 unit + 0 integration tests)

## Gotchas

### Patterns (apply across modules)

- Tower middleware pattern: `Layer` + `Service` structs, manual `Clone` impls, `std::mem::swap` in `call()` to preserve ready service — see `src/tenant/middleware.rs` as reference
- RPITIT traits (OAuthProvider, TenantResolver, RoleExtractor) — not object-safe; use concrete types
- Internal traits behind `Arc<dyn Trait>` must use `Pin<Box<dyn Future>>` returns (not RPITIT) to stay object-safe — see `DnsResolver` in `src/dns/resolver.rs`
- New middleware traits that need session access must take `&mut Parts` (not `&Parts`) so they can call `Session::from_request_parts()` — `SessionState` is `pub(crate)`
- Guard/middleware errors use `Error::into_response()` — never construct raw HTTP responses; errors flow through the app's custom error handler
- Always-available modules (no feature gate): cache, encoding, flash, ip, session, tenant, rbac, job, cron, testing (`test-helpers` feature)
- `std::sync::RwLock` (not tokio) for all sync-only state — never hold across `.await`
- Feature-gated modules: test with `cargo test --features X`, lint with `cargo clippy --features X --tests`, integration test files need `#![cfg(feature = "X")]`
- No self-referencing dev-dependencies for feature-gated tests — use `#![cfg(feature = "X")]` guards and run via `cargo test --features X`
- Types without `Debug` (pool newtypes, `Storage`, `Buckets`): use `.err().unwrap()` not `.unwrap_err()` in tests
- `Error`'s `Clone` and `IntoResponse` both drop `source` (can't clone `Box<dyn Error>`) — use `error_code: Option<&'static str>` field to preserve error identity through the response pipeline
- `Error::with_source(status, msg, source)` is a constructor (3 args) — the builder-style method is `chain(source)` (1 arg); don't confuse them
- Error identity pattern: `Error::unauthorized("unauthorized").chain(JwtError::Expired).with_code(JwtError::Expired.code())` — `source_as::<T>()` for pre-response, `error_code()` for post-response
- `Arc<Inner>` pattern (Engine, Broadcaster, Storage, GeoLocator) — `Inner` struct and field must be private (not `pub(crate)`); never double-wrap in `Arc`
- Conditionally-used items: `#[cfg_attr(not(any(test, feature = "X-test")), allow(dead_code))]`; modules imported behind `cfg` need `pub(crate) mod`
- Feature-gated modules accessed by integration tests (`tests/*.rs`) must use `pub mod` not `pub(crate) mod` — integration tests are external crate consumers
- `Cargo.lock` is gitignored (library crate) — don't stage it in commits

### Rust 2024 / Tooling

- Rust 2024 prelude includes `Future` — no `use std::future::Future` needed anywhere (RPITIT, `Pin<Box<dyn Future>>`, etc.)
- `std::env::set_var` / `remove_var` are `unsafe` — tests must wrap in `unsafe {}` blocks
- Config tests that modify env vars must use `serial_test` to avoid races
- Tests that modify env vars must clean up BEFORE assertions — panics skip cleanup
- `cargo clippy --tests` needed to lint test code (plain `cargo clippy` skips it)
- Clippy rejects `mod foo` inside `foo/mod.rs` — name the file differently
- `cargo tree -p <pkg>` fails behind feature flags — use `cargo tree --invert <pkg>` instead
- Clippy `manual_div_ceil`: use `n.div_ceil(d)` not `(n + d - 1) / d` — flagged since Rust 1.92
- Clippy `io_other_error`: use `io::Error::other("msg")` not `io::Error::new(io::ErrorKind::Other, "msg")` — flagged since Rust 1.92
- Rust 2024 edition rejects explicit `ref` in `if let` patterns through references — use `if let (Some(x), ...)` not `if let (Some(ref x), ...)`
- Clippy `collapsible_if`: nested `if let` + `if` must use let-chains — `if let Some(x) = y && condition {}`

### axum

- Handler functions inside `#[tokio::test]` closures don't satisfy `Handler` bounds — define as module-level `async fn`
- axum 0.8: `OptionalFromRequestParts` needs explicit impl for `Option<MyExtractor>`
- `PathParamStrategy` requires `.route_layer()` not `.layer()` — path params only exist after route matching
- `RawPathParams` depends on internal `UrlParams` — positive tests need real `Router` + `oneshot`
- Adding fields to `Error` requires updating ALL struct literal sites (including `IntoResponse` copy)
- `Router::layer()` bounds: `L` and `L::Service` both need `+ Sync`, error must be `Into<Infallible>` (not `Into<Box<dyn Error>>`)

### SQLite

- No `ON CONFLICT` with partial unique indexes — use plain `INSERT` and catch `is_unique_violation()`
- Worker poll loop: 999 bind params limit — max ~900 registered handlers

### Dependencies

- `base64` crate for standard base64 (webhooks feature) — NOT `encoding::base64url` which is RFC 4648 no-padding
- YAML deserialization uses `serde_yaml_ng` — NOT `serde_yaml` (different crate)
- `run!` macro uses `$crate::tracing::info!` for hygiene — regular code uses bare `tracing::`
- `rand::fill(&mut bytes)` not `rand::rng().fill_bytes()` (latter needs `use rand::Rng`)
- `croner::Cron::new()` defaults to 5-field — call `.with_seconds_optional()` for 6-field
- `hyper-rustls` needs `webpki-roots` feature for `.with_webpki_roots()`
- Session middleware uses raw `cookie::CookieJar` — NOT `axum_extra::extract::cookie::SignedCookieJar`
- `SessionLayer` must be re-exported from `src/session/mod.rs` — needed by test helpers and any code that programmatically creates session layers
- MiniJinja: URLs/HTML must use `Value::from_safe_string()`; registrations consume by move (`Box<dyn FnOnce>`)
- `simple-dns` 0.11 for DNS packet parsing (dns feature) — `TXT::attributes()` returns `HashMap<String, Option<String>>`, `CNAME` is tuple struct `CNAME(pub Name<'a>)`
- `maxminddb` 0.27 for geolocation — two-step API: `reader.lookup(ip)` → `LookupResult`, then `.decode::<T>()` returns `Option<T>`. Error enum is `MaxMindDbError` (not `MaxMindDBError`), `#[non_exhaustive]`. `geoip2::City` has non-optional nested structs with typed `Names` having `.english: Option<&str>` (not `BTreeMap` — don't use `.get("en")`)

### Storage

- S3 keys: always URI-encode with `uri_encode(key, false)` — omitting breaks keys with spaces/`+`
- `delete_prefix()` is O(n) network calls — not for large prefixes
- Hand-parsed XML for ListObjectsV2 — switch to `quick-xml` if parsing breaks
- Streaming body reads: use `BodyExt::frame()` loop from `http_body_util`, NOT `body.collect().await` — collect buffers everything, defeating mid-stream abort on size limit
- `http_body` is a transitive dep (via hyper/axum) — use `http_body_util::BodyExt` for `.frame()`, no need to add `http-body` to Cargo.toml
- `pub(crate)` functions can't be tested from `tests/*.rs` — HTTP server tests for internal functions must be unit tests in the source file
- `x-amz-acl` header may be silently ignored by S3-compatible providers (RustFS/MinIO) if ACLs are disabled at server level
- `put_from_url()` does not follow redirects (SSRF prevention) — callers must provide the final URL
- `put_from_url()` has a hard-coded 30s timeout — wraps the fetch in `tokio::time::timeout`
- `put_from_url()` on memory backend returns `Error::internal` — it's inherently a network operation, use unit tests in `fetch.rs` for HTTP server tests

### Rate Limiting

- `rate_limit()` and `rate_limit_with()` require a `CancellationToken` — cleanup task shuts down when token is cancelled
- `ShardedMap::check_or_insert()` counts total keys across all shards to enforce `max_keys` — this takes read locks on all shards briefly

### Design Decisions

- RBAC is roles-only (no permissions model) — app handles permissions in handler logic
- Job priority is handled by separate queues/worker pools, not numeric priority in a single queue
- `.env` loading is the end-app's responsibility (via Justfile) — framework only does YAML config with `${VAR}` substitution
- DB-backed modules (session, job) don't ship migrations — end-apps own their schemas
- `TenantId::ApiKey` must be redacted in Display/Debug — never log raw API keys
- `tracing()` middleware must declare `tenant_id = tracing::field::Empty` for tenant middleware to `record()` later
- `todo!()` stubs need `#[allow(dead_code)]` to pass clippy — remove when implementing
- Use official documentation only when researching dependencies
