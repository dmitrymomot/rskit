# ADR: Split modo Into Core + Extension Crates

> **Priority: HIGHEST — must be implemented before any other work.**

## Status

Accepted (2026-03-06)

## Context

The current monolithic `modo` crate bundles everything: HTTP, DB, sessions, auth, jobs, templates, CSRF. Every change ripples across the entire framework. This causes:

1. **Compile time waste** — users pay for session/jobs/templates even if unused
2. **Rigidity** — users can't swap session/auth/jobs implementations
3. **Maintenance burden** — independent features are coupled in one crate
4. **Developer fatigue** — any change requires fixes across the whole framework

## Decision

Split modo into a minimal core crate + independent extension crates (Approach C: extension-to-extension deps, no core traits for session/auth/jobs).

### Core (`modo`)

Stable foundation that rarely changes:

- axum HTTP server, app builder, lifecycle (graceful shutdown)
- DB connection, SeaORM, entity-first migrations, schema sync
- Cookie jar (signed + private) — a primitive, not a feature
- Service registry + `Service<T>` extractor
- `Db` extractor
- Error types
- Config loading (env + .env)
- Router + `inventory` auto-discovery
- `#[handler]`, `#[main]`, `#[module]`, `#[entity]`, `#[migration]` macros
- Re-exports (axum, sea_orm, etc.)

### Extension Crates

| Crate | Contains | Depends on |
|---|---|---|
| `modo-session` | Session types, store trait+impl, manager, middleware, fingerprint, device | `modo` (core) |
| `modo-auth` | `UserProvider` trait, `Auth<User>`, `OptionalAuth<User>` extractors | `modo`, `modo-session` |
| `modo-jobs` | Queue, runner, cron, job entity/store, `#[job]` macro | `modo` (core) |
| `modo-templates` | Askama integration, BaseContext, flash, HTMX helpers, `#[context]` macro | `modo` (core) |
| `modo-csrf` | CSRF double-submit cookie middleware | `modo` (core) |

### Monorepo Structure

All crates live in one Cargo workspace (standard Rust practice for related crates):

```
modo/
  Cargo.toml          # workspace root
  modo/               # core
  modo-macros/        # proc macros
  modo-session/
  modo-auth/
  modo-jobs/
  modo-templates/
  modo-csrf/
```

### End-User DX

```toml
# Cargo.toml
[dependencies]
modo = "0.1"
modo-session = "0.1"    # opt-in
modo-auth = "0.1"       # opt-in
```

```rust
use modo::prelude::*;
use modo_session::{SessionManager, SqliteSessionStore};

#[modo::main]
async fn main(app: modo::App) {
    let session_store = SqliteSessionStore::new(app.db()).await;

    app.service(session_store)
        .middleware(modo_session::layer())
        .run()
        .await
}

#[modo::handler(GET, "/dashboard")]
async fn dashboard(session: SessionManager) -> Result<String> {
    let user_id: Option<String> = session.get("user_id").await?;
    match user_id {
        Some(id) => Ok(format!("Welcome back, {id}")),
        None => Ok("Not logged in".into()),
    }
}
```

Extension crates integrate via the existing service registry + middleware + extractor pattern. Core knows nothing about sessions, auth, or jobs.

## Consequences

**Positive:**
- Core becomes a rock — almost never needs to change
- "Fix one thing, touch everything" disappears
- Users only compile what they use
- Extensions evolve and version independently
- Matches Rust ecosystem conventions (axum/axum-extra/axum-login pattern)

**Negative:**
- Users add multiple crates to `Cargo.toml`
- Extension-to-extension version compatibility must be managed
- Some proc macros (`#[job]`, `#[context]`) move to extension macro crates or the extension crates themselves

## Implementation Order

1. Extract `modo-session` (largest, most coupled module)
2. Extract `modo-auth` (depends on modo-session)
3. Extract `modo-jobs`
4. Extract `modo-templates`
5. Extract `modo-csrf`
6. Clean up core — remove all feature flags for extracted modules
