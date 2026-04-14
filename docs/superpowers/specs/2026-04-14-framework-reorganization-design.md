# modo v0.7 — Framework Reorganization Design

**Status:** approved design, ready for implementation planning
**Target version:** 0.7.0 (breaking release)
**Authors:** brainstorming session 2026-04-14

## Problem

The current module layout (v0.6.3) has accumulated three categories of placement smells:

1. **Identity logic is scattered across five top-level modules.** `auth` holds credentials/JWT/OAuth, `session` holds HTTP sessions, `apikey` holds API-key auth, `rbac` holds role checks, and `rbac` also owns `require_authenticated` — a gate about identity _presence_, not roles. No single module owns the concept of "authenticated user", so the gate lives in the closest neighbour.

2. **Two top-level modules are grab-bags.** `middleware/` mixes CORS, CSRF, compression, rate limiting, security headers, request IDs, tracing, panic handling, and error handling into a single flat file tree. `extractor/` mixes deserialization extractors (`JsonRequest`, `FormRequest`, `Query`, `MultipartRequest`) with service-registry access (`Service`) and client-context extraction (`ClientInfo`).

3. **Feature flags do not serve the actual user.** The framework ships 14 feature flags to support hypothetical users who want minimal subsets. In practice every project built on modo enables roughly the same 11 features plus `test-helpers`. The flags produce `#[cfg(feature = "…")]` noise in both modo's source and application code, fragment documentation across feature-gated sections, and add version-bump bookkeeping without paying rent.

These three issues compound: the identity sprawl is hidden by feature gating (`#[cfg(feature = "session")]` hides how session depends on auth), the middleware grab-bag is the model a reader generalises to when understanding the framework, and the extractor grab-bag makes "where does request X come from" a multi-folder search.

## Goals

- Make the home of every concept predictable from its semantic role.
- Delete organisational overhead that exists for a constituency the framework does not have.
- Preserve ergonomic flat access at wiring sites without sacrificing source cohesion.
- Fix the specific inconsistency that motivated this redesign: `require_authenticated` living in `rbac`.

## Non-goals

- No module deletions. All 37 current top-level modules continue to exist as either top-level modules or submodules of an umbrella. `embed`, `qrcode`, `dns`, `tier`, `apikey`, `audit`, `geolocation` all stay.
- No workspace split. modo remains a single crate with explicit wiring.
- No renames beyond the `rbac` → `role` change required for honest naming.
- No deep umbrellas beyond `auth/`. `http/`, `persist/`, `bg/` and similar are explicitly rejected as cosmetic.

## Design

### 1. `auth/` umbrella — identity consolidated

`auth/` becomes the single home for every identity, credential, token, and access concept. Four of today's top-level modules move inside it:

```
src/auth/
  mod.rs
  guard.rs                 ← require_authenticated, require_role, require_scope
                             (the single gating surface for route-level access control)
  password.rs
  otp.rs
  totp.rs
  backup.rs
  jwt/
    claims.rs  config.rs  decoder.rs  encoder.rs  error.rs
    middleware.rs  revocation.rs  signer.rs  source.rs  validation.rs
  oauth/
    config.rs  client.rs  github.rs  google.rs
    profile.rs  provider.rs  state.rs
  session/                 ← was src/session/
    mod.rs  store.rs  extractor.rs  middleware.rs  config.rs
    fingerprint.rs  device.rs  meta.rs  token.rs
  apikey/                  ← was src/apikey/
    mod.rs  store.rs  layer.rs  config.rs  scope.rs
  role/                    ← was src/rbac/ (renamed)
    mod.rs  middleware.rs  extractor.rs
```

**Key moves:**

- `src/rbac/guard.rs` → `src/auth/guard.rs`, expanded to host `require_authenticated`, `require_role`, and `require_scope` (the last previously lived in `apikey`). This is the single place for route-level identity gating regardless of the identity source.
- `src/session/` → `src/auth/session/`.
- `src/apikey/` → `src/auth/apikey/`.
- `src/rbac/` → `src/auth/role/`. Renamed because modo's module is roles-only (permissions are handler-logic per CLAUDE.md); "rbac" was misleading jargon.

The `auth::role` module keeps only `Role`, `RoleExtractor`, and the middleware that populates `Role` in extensions. All guarding moves to `auth::guard`.

### 2. `middleware/` and `extractor/` — internal structure

Both modules keep their top-level name but gain proper submodule structure. Today they are grab-bags; they become organised menus.

```
src/middleware/
  mod.rs                   (re-exports the constructors)
  cors.rs
  csrf.rs
  compression.rs
  rate_limit.rs
  security_headers.rs
  request_id.rs
  tracing.rs
  panic.rs
  error_handler.rs

src/extractor/
  mod.rs
  json.rs                  (JsonRequest)
  form.rs                  (FormRequest)
  query.rs                 (Query)
  multipart.rs             (MultipartRequest, UploadedFile)
```

Two types move out of `extractor/` to modules that own the concept:

- `extractor::Service` → `service::Registry`. This type is the service registry; it was never an extractor in any meaningful sense, and the name `Registry` describes what it is.
- `extractor::ClientInfo` → `ip::ClientInfo`. Client context and client IP are the same topic and share header-parsing logic.

### 3. Virtual layer modules — flat wiring ergonomics

Three new re-export-only modules expose every middleware/extractor/guard in a single flat surface, without duplicating source:

```rust
// src/middlewares.rs
pub use crate::auth::session::layer    as session;
pub use crate::auth::jwt::layer        as jwt;
pub use crate::auth::apikey::layer     as api_key;
pub use crate::auth::role::middleware  as role;
pub use crate::tenant::layer           as tenant;
pub use crate::tier::layer             as tier;
pub use crate::ip::layer               as client_ip;
pub use crate::flash::layer            as flash;
pub use crate::geolocation::layer      as geo;
pub use crate::template::context_layer as template_context;
pub use crate::middleware::{
    cors, csrf, compression, rate_limit, security_headers,
    request_id, tracing, panic, error_handler,
};

// src/extractors.rs
pub use crate::extractor::{JsonRequest, FormRequest, Query, MultipartRequest, UploadedFile};
pub use crate::auth::session::Session;
pub use crate::auth::jwt::{Bearer, Claims};
pub use crate::auth::apikey::ApiKey;
pub use crate::auth::role::Role;
pub use crate::tenant::Tenant;
pub use crate::tier::TierInfo;
pub use crate::ip::{ClientIp, ClientInfo};
pub use crate::flash::Flash;
pub use crate::template::HxRequest;
pub use crate::sse::LastEventId;
pub use crate::service::AppState;

// src/guards.rs
pub use crate::auth::guard::{require_authenticated, require_role, require_scope};
pub use crate::tier::{require_feature, require_limit};
```

These files contain pure re-exports. They give router-wiring code a flat menu (`middlewares::session(...)`, `guards::require_role([...])`) without pulling any implementation out of its domain home.

### 4. Zero feature flags

All capability feature flags are deleted. The only remaining flag is `test-helpers`.

```toml
[features]
default = []
test-helpers = ["dep:serial_test", "dep:tempfile"]
```

Deleted flags: `db`, `session`, `job`, `auth`, `templates`, `sse`, `email`, `storage`, `webhooks`, `dns`, `geolocation`, `qrcode`, `sentry`, `apikey`, `text-embedding`, `tier`.

Every module is always compiled. Heavy transitive dependencies (`maxminddb`, `sentry`, LLM HTTP clients for `embed`, AWS SDK for `storage`, `lettre` for `email`) become unconditional.

Application `Cargo.toml` becomes `modo = { package = "modo-rs", version = "0.7" }` — no feature list.

All `#[cfg(feature = "…")]` attributes in modo source are deleted except those gating `test-helpers`.

### 5. Prelude-driven public API

Flat re-exports at crate root are removed except for `Error`, `Result`, `Config`, and the dependency re-exports (`axum`, `serde`, `serde_json`, `tokio`). Common handler-time types live in a new `modo::prelude` module.

```rust
// src/prelude.rs
pub use crate::error::{Error, Result};
pub use crate::service::AppState;

pub use crate::auth::session::Session;
pub use crate::auth::role::Role;

pub use crate::flash::Flash;
pub use crate::ip::ClientIp;
pub use crate::tenant::{Tenant, TenantId};
pub use crate::validate::{Validate, ValidationError, Validator};
```

Extractors are deliberately excluded from the prelude: `Query` and similar names risk collision with application code, and explicit imports (`use modo::extractor::JsonRequest;`) match axum idiom.

Domain types (`Claims`, `Mailer`, `Storage`, `Engine`, etc.) are reached through their module paths (`modo::auth::jwt::Claims`, `modo::email::Mailer`, etc.) and are deliberately not preluded — they are feature-specific and shallow paths would hide where they belong.

### 6. `lib.rs`

```rust
//! modo — a Rust web framework for small monolithic apps.

pub mod error;
pub mod config;
pub mod runtime;
pub mod server;
pub mod service;

pub mod db;
pub mod cache;
pub mod storage;

pub mod cookie;
pub mod flash;
pub mod ip;
pub mod sse;
pub mod middleware;
pub mod extractor;

pub mod auth;
pub mod tenant;
pub mod tier;

pub mod job;
pub mod cron;

pub mod email;
pub mod webhook;
pub mod template;
pub mod qrcode;

pub mod tracing;
pub mod audit;
pub mod health;

pub mod dns;
pub mod geolocation;
pub mod embed;

pub mod validate;
pub mod id;
pub mod encoding;
pub mod sanitize;

#[cfg(feature = "test-helpers")]
pub mod testing;

pub mod prelude;
pub mod middlewares;
pub mod extractors;
pub mod guards;

pub use config::Config;
pub use error::{Error, Result};

pub use axum;
pub use serde;
pub use serde_json;
pub use tokio;
```

### 7. Complete target module list

34 regular top-level modules (was 37 — three absorbed into `auth/`: `session`, `apikey`, `rbac`). Plus four new re-export-only indexes: `prelude`, `middlewares`, `extractors`, `guards`.

**Framework core** — `error`, `config`, `runtime`, `server`, `service`
**Persistence** — `db`, `cache`, `storage`
**HTTP plumbing** — `cookie`, `flash`, `ip`, `sse`, `middleware`, `extractor`
**Identity (umbrella)** — `auth` (absorbs `session`, `apikey`, `rbac`→`role`; already housed `jwt`, `oauth`, `password`, `otp`, `totp`, `backup`)
**Multi-tenancy** — `tenant`, `tier`
**Background work** — `job`, `cron`
**Output** — `email`, `webhook`, `template`, `qrcode`
**Observability** — `tracing`, `audit`, `health`
**Specialty services** — `dns`, `geolocation`, `embed`
**Input & utilities** — `validate`, `id`, `encoding`, `sanitize`
**Testing** — `testing` (gated by `test-helpers`)
**Virtual indexes** — `prelude`, `middlewares`, `extractors`, `guards`

## User-visible impact

### Application `Cargo.toml`

```toml
# Before (v0.6.3)
modo = { package = "modo-rs", version = "0.6", features = [
  "db", "session", "job", "templates", "auth", "email",
  "storage", "sse", "webhooks", "geolocation", "sentry",
] }

# After (v0.7)
modo = { package = "modo-rs", version = "0.7" }
```

### Handler imports

```rust
// Before
use modo::{Error, Result, Session, Flash, ClientIp, Role, Validator};
use modo::tenant::{Tenant, TenantId};
use modo::auth::jwt::Claims;
use modo::extractor::JsonRequest;
#[cfg(feature = "templates")]
use modo::template::Renderer;

// After
use modo::prelude::*;
use modo::extractor::JsonRequest;
use modo::auth::jwt::Claims;
use modo::template::Renderer;
```

### Router wiring

```rust
// Before
Router::new()
    .route("/admin", get(admin))
    .route_layer(modo::rbac::require_role(["admin"]))
    .layer(modo::session::layer(store, &cookie_cfg, &key))
    .layer(modo::auth::jwt::layer(decoder))
    .layer(modo::tenant::middleware(resolver))
    .layer(modo::middleware::cors(cors_cfg))
    .layer(modo::middleware::rate_limit(100))

// After
use modo::{middlewares as mw, guards};

Router::new()
    .route("/admin", get(admin))
    .route_layer(guards::require_role(["admin"]))
    .layer(mw::session(store, &cookie_cfg, &key))
    .layer(mw::jwt(decoder))
    .layer(mw::tenant(resolver))
    .layer(mw::cors(cors_cfg))
    .layer(mw::rate_limit(100))
```

### Relocation cheat sheet

| Before                                            | After                                                               |
| ------------------------------------------------- | ------------------------------------------------------------------- |
| `modo::Session`                                   | `modo::prelude::Session` (or `modo::auth::session::Session`)        |
| `modo::SessionLayer`, `modo::session::layer`      | `modo::middlewares::session` (or `modo::auth::session::layer`)      |
| `modo::session::Store`                            | `modo::auth::session::Store`                                        |
| `modo::apikey::*`                                 | `modo::auth::apikey::*`                                             |
| `modo::rbac::Role`                                | `modo::auth::role::Role`                                            |
| `modo::rbac::require_role`                        | `modo::auth::guard::require_role` (or `modo::guards::require_role`) |
| `modo::rbac::require_authenticated`               | `modo::auth::guard::require_authenticated`                          |
| `modo::apikey::require_scope`                     | `modo::auth::guard::require_scope`                                  |
| `modo::extractor::Service`                        | `modo::service::Registry`                                           |
| `modo::ClientInfo`, `modo::extractor::ClientInfo` | `modo::ip::ClientInfo`                                              |
| `modo::middleware::cors`                          | unchanged (or `modo::middlewares::cors`)                            |
| `modo::JwtLayer`, `modo::Claims`, etc.            | `modo::auth::jwt::*` (no crate-root re-export)                      |
| `modo::GitHub`, `modo::Google`, etc.              | `modo::auth::oauth::*` (no crate-root re-export)                    |

## Internal impact on modo source

- Delete every `#[cfg(feature = "…")]` attribute except those gating `testing` (`test-helpers`).
- Delete the `default` feature and every per-capability feature from `Cargo.toml`.
- Move four directories as described above (`session/`, `apikey/`, `rbac/` → `auth/role/`, plus the `src/rbac/guard.rs` → `src/auth/guard.rs` extraction).
- Split `src/middleware/` (currently flat files) into one file per concern.
- Split `src/extractor/` into one file per extractor type.
- Move `src/extractor/service.rs` → `src/service/registry.rs` (with rename `Service` → `Registry`).
- Move `src/extractor/client_info.rs` → `src/ip/client_info.rs`.
- Rewrite `lib.rs` to reflect the new layout and delete flat re-exports except essentials.
- Add `src/prelude.rs`, `src/middlewares.rs`, `src/extractors.rs`, `src/guards.rs` as re-export files.
- Update every module-level `//!` doc comment, every `src/**/README.md`, and the root `README.md` to reflect new paths.
- Update `skills/dev/references/*.md` in the modo-dev skill plugin to use new paths.
- Bump version to `0.7.0` in `Cargo.toml`, `.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`.
- Migrate existing tests mechanically alongside the reorg (update `use` paths, drop feature-gate attributes). Do **not** rewrite or delete tests in this release — they are the only safety net proving the reorg preserves behaviour. A dedicated test audit and coverage rewrite is out of scope for 0.7 and lands as a follow-up release.
- Skills overhaul (the `modo:dev` / `modo:init` / `modo:deploy` plugin skills) is explicitly out of scope for 0.7 and deferred to a separate brainstorming session after 0.7 ships. Any reference to modo paths in those skills will be updated mechanically as part of this release but no structural skill redesign.

## Rejected alternatives

**Deep domain umbrellas (`identity/`, `http/`, `persist/`, `bg/`).** Only the identity umbrella pays rent — the others are cosmetic and would push common paths deeper (`modo::persist::db::Database`) without cure for any concrete smell.

**Layer-based organisation (`middlewares/`, `extractors/`, `guards/`, `stores/` as physical source directories).** Session middleware is 358 lines of session logic, not Tower boilerplate; moving it to `middlewares/session.rs` splits session's cohesive file set across two top-level directories and turns every domain into a cross-folder treasure hunt. The virtual re-export modules (section 3) give the flat wiring ergonomics without the cohesion cost.

**Per-submodule feature flags after reorg (keeping `auth`, `session`, `apikey` as separate flags).** Doesn't match the framework's actual user pattern. Every real project enables the full set; flags add bookkeeping for a constituency that does not exist.

**Profile flags (`monolith`, `lite`, `heavy`).** One bit of Cargo.toml information that always resolves to the same value. Cleverness without payoff.

**Merge `id` + `encoding` + `sanitize` into `util/`.** `modo::id::ulid()` is called constantly in handlers and ID-generation code. Forcing `modo::util::id::ulid()` adds a path segment for every call site in exchange for saving two top-level module names. Ergonomics wins.

## Open questions

None. Guard functions keep the `require_*` prefix (`require_authenticated`, `require_role`, `require_scope`, `require_feature`, `require_limit`) — shorter than `allow_for_*` and matches axum convention.

## Rollout

Single breaking release as `0.7.0`. All downstream applications update imports in one pass using the relocation cheat sheet above. Migration order and implementation plan are the subject of a separate document produced by the `writing-plans` skill.
