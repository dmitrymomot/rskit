# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

modo — Rust web framework. Single crate, zero proc macros, plain `async fn` handlers, axum Router, libsql (SQLite), explicit wiring. Rust 2024 edition, MSRV 1.92.

## Commands

- `cargo check` — type check
- `cargo test` — run all tests
- `cargo test --features X` — test feature-gated module
- `cargo clippy --features X --tests -- -D warnings` — lint (plain `cargo clippy` skips test code)
- `cargo fmt` / `cargo fmt --check` — format

## Workflow

- `superpowers:brainstorming` skill before implementation
- `superpowers:subagent-driven-development` skill for plan execution

## Conventions

- NEVER use absolute paths — always relative to project root
- `mod.rs` / `lib.rs` are ONLY for `mod` imports and re-exports — all code in separate files
- `modo::Error` with status + message + optional source; `modo::Result<T>`; `?` everywhere
- Error constructors: `Error::not_found()`, `Error::bad_request()`, `Error::internal()`, etc.
- `Error::with_source(status, msg, src)` is a constructor — builder method is `.chain(src)`
- Error identity: `.chain(e).with_code(e.code())` — `source_as::<T>()` pre-response, `error_code()` post-response
- `Error` clone/response both drop `source` — use `error_code: Option<&'static str>` for identity across response boundary
- IDs: `id::ulid()` (26 chars) or `id::short()` (13 chars, base36) — no UUID
- Pluggable backends: `Arc<dyn Trait>` (not `Box`)
- `Arc<Inner>` pattern — `Inner` struct/field must be private; never double-wrap
- `std::sync::RwLock` (not tokio) for sync-only state — never hold across `.await`
- Tracing fields: snake_case (`user_id`, `session_id`)
- Config: YAML with `${VAR}` / `${VAR:default}` env substitution; `trusted_proxies` is top-level
- Database: single `Database` handle (`Arc<Connection>`); `connect()` opens one connection with PRAGMA defaults; `ConnExt` for raw queries, `ConnQueryExt` for typed helpers; `libsql::params!` for bind parameters
- No TODOs, no workarounds — every declared field and API must be fully implemented

## Feature Flags

Feature-gated modules: `auth`, `templates`, `sse`, `email`, `storage`, `webhooks`, `dns`, `geolocation`, `qrcode`, `sentry`. Always-available: cache, encoding, flash, ip, session, tenant, rbac, job, cron, testing (`test-helpers`).

- Integration test files need `#![cfg(feature = "X")]`
- Feature-gated modules for integration tests must use `pub mod` (not `pub(crate) mod`)
- Companion test features (`X-test`) for dev-only code: `#[cfg_attr(not(any(test, feature = "X-test")), allow(dead_code))]`
- `Cargo.lock` is gitignored (library crate)

## Gotchas

### Middleware & Traits

- Tower middleware: `Layer` + `Service`, manual `Clone`, `std::mem::swap` in `call()` — see `src/tenant/middleware.rs`
- RPITIT traits (OAuthProvider, TenantResolver, RoleExtractor) are not object-safe — use concrete types
- Traits behind `Arc<dyn Trait>` must use `Pin<Box<dyn Future>>` (not RPITIT) — see `src/dns/resolver.rs`
- Middleware needing session: take `&mut Parts` so `Session::from_request_parts()` works
- Guard/middleware errors use `Error::into_response()` — never construct raw HTTP responses

### axum 0.8

- Handlers in `#[tokio::test]` closures don't satisfy `Handler` bounds — use module-level `async fn`
- `Option<MyExtractor>` needs explicit `OptionalFromRequestParts` impl
- Path-dependent layers: `.route_layer()` not `.layer()`
- `Router::layer()`: `L` and `L::Service` need `+ Sync`, error `Into<Infallible>`

### Rust 2024

- Prelude includes `Future` — no `use std::future::Future` needed
- `set_var`/`remove_var` are `unsafe`; env-var tests need `serial_test` and must clean up BEFORE assertions
- `mod foo` inside `foo/mod.rs` rejected by clippy — name file differently
- Let-chains required for nested `if let` + `if`

### SQLite

- No `ON CONFLICT` with partial unique indexes — `INSERT` + catch `is_unique_violation()`
- 999 bind params limit per query

### Dependencies

- YAML: `serde_yaml_ng` (not `serde_yaml`)
- base64: `base64` crate for standard, `encoding::base64url` for RFC 4648 no-padding
- rand: `rand::fill(&mut bytes)` not `rand::rng().fill_bytes()`
- croner: `.with_seconds_optional()` for 6-field cron
- Session: raw `cookie::CookieJar`, not `axum_extra` signed jar
- MiniJinja: `Value::from_safe_string()` for URLs/HTML; registrations consume by move
- Streaming HTTP: `BodyExt::frame()` loop, not `body.collect().await`
- S3 keys: always `uri_encode(key, false)`

### Design Decisions

- RBAC is roles-only — app handles permissions in handler logic
- Job priority via separate queues, not numeric priority
- DB-backed modules don't ship migrations — end-apps own schemas
- `TenantId::ApiKey` must be redacted in Display/Debug
- `tracing()` middleware pre-declares `tenant_id = tracing::field::Empty` so tenant middleware can `record()` into it — new fields that middleware needs to fill must be added to `ModoMakeSpan`
- Use official docs only when researching dependencies

### Test Fixtures

- `tests/fixtures/migrations/` — `TestDb::migrate()` tests
- `tests/fixtures/GeoIP2-City-Test.mmdb` — geolocation tests
- Types without `Debug` (pool newtypes, Storage): `.err().unwrap()` not `.unwrap_err()`
