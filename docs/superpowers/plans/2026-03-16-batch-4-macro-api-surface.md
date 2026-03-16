# Batch 4: Macro & API Surface — Implementation Plan

> **Status: COMPLETE** — All 2 issues (INC-18, INC-13) implemented and merged in PR `fix/review-issues`.

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Standardize all proc-macro crate re-exports behind `#[doc(hidden)] pub mod __internal` modules and eliminate duplicate ULID-based ID boilerplate via a shared `ulid_id!` macro.

**Architecture:** Each parent crate (`modo`, `modo-db`, `modo-jobs`, `modo-upload`) will expose a single `#[doc(hidden)] pub mod __internal` that contains every type and re-export its proc macros reference in generated code. All proc macros will be updated to emit `<crate>::__internal::*` paths exclusively. A declarative `ulid_id!` macro in `modo` core will replace the hand-rolled `SessionId` and `JobId` types.

**Tech Stack:** Rust proc_macro2/quote/syn, inventory, serde, ulid, SeaORM (optional for DB-backed IDs)

---

## Prerequisites

- **INC-15** (Batch 1) must be merged first — it renames `ContextLayer` to `TemplateContextLayer`, which affects the template-related paths consolidated into `modo::__internal`.

---

## Item 1: INC-18 — Standardize macro re-exports on `pub mod __internal`

### Step 1.1: Audit — Catalog every external path referenced in generated code

Below is the complete catalog of paths each proc-macro crate emits in generated code. These are the paths that must be consolidated into `__internal` modules.

#### modo-macros (parent crate: `modo`)

| Macro | Paths referenced in generated code |
|---|---|
| `#[handler]` | `modo::serde::Deserialize`, `modo::serde` (serde crate attr), `modo::axum::extract::Path`, `modo::router::Method::{GET,POST,PUT,PATCH,DELETE,HEAD,OPTIONS}`, `modo::axum::routing::{get,post,put,patch,delete,head,options}`, `modo::inventory::submit!`, `modo::router::RouteRegistration`, `modo::router::MiddlewareFn`, `modo::axum::middleware::from_fn`, `modo::axum::routing::MethodRouter`, `modo::app::AppState` |
| `#[module]` | `modo::inventory::submit!`, `modo::router::ModuleRegistration`, `modo::router::RouterMiddlewareFn`, `modo::axum::Router`, `modo::app::AppState`, `modo::axum::middleware::from_fn` |
| `#[error_handler]` | `modo::inventory::submit!`, `modo::error::ErrorHandlerRegistration`, `modo::error::ErrorHandlerFn` |
| `#[view]` | `::modo::serde::Serialize`, `::modo::serde` (serde crate attr), `::modo::axum::response::IntoResponse`, `::modo::axum::response::Response`, `::modo::minijinja::Value`, `::modo::templates::View`, `::modo::templates::ViewRender`, `::modo::templates::TemplateEngine`, `::modo::templates::TemplateContext`, `::modo::templates::TemplateError` |
| `#[template_function]` | `::modo::inventory::submit!`, `::modo::templates::TemplateFunctionEntry`, `::modo::minijinja::Environment` |
| `#[template_filter]` | `::modo::inventory::submit!`, `::modo::templates::TemplateFilterEntry`, `::modo::minijinja::Environment` |
| `#[main]` | `modo::tokio::runtime::Builder`, `modo::tracing_subscriber::EnvFilter`, `modo::tracing_subscriber::fmt`, `modo::app::AppBuilder`, `modo::config::load_or_default`, `modo::tracing::error!`, `::modo::rust_embed` |
| `Sanitize` (derive) | `modo::sanitize::Sanitize`, `modo::sanitize::SanitizerRegistration`, `modo::sanitize::{trim,lowercase,uppercase,strip_html_tags,collapse_whitespace,truncate,normalize_email}`, `modo::inventory::submit!` |
| `Validate` (derive) | `modo::validate::Validate`, `modo::validate::validation_error`, `modo::validate::is_valid_email`, `modo::Error` |

#### modo-db-macros (parent crate: `modo-db`)

| Macro | Paths referenced in generated code |
|---|---|
| `#[entity]` | `modo_db::generate_ulid`, `modo_db::generate_nanoid`, `modo_db::chrono::{DateTime,Utc}`, `modo_db::sea_orm::{entity::prelude::*, ActiveValue, ActiveModelTrait, EntityTrait, QueryFilter, ColumnTrait, ConnectionTrait, DeriveEntityModel, EnumIter, DeriveRelation, sea_query::Expr}`, `modo_db::sea_orm::entity::prelude::Related`, `modo_db::inventory::submit!`, `modo_db::EntityRegistration`, `modo_db::db_err_to_error`, `modo_db::Record`, `modo_db::DefaultHooks`, `modo_db::do_insert`, `modo_db::do_update`, `modo_db::do_delete`, `modo_db::EntityQuery`, `modo_db::EntityUpdateMany`, `modo_db::EntityDeleteMany`, `modo::Error`, `modo::HttpError::NotFound` |
| `#[migration]` | `modo_db::inventory::submit!`, `modo_db::MigrationRegistration` |

#### modo-jobs-macros (parent crate: `modo-jobs`)

| Macro | Paths referenced in generated code |
|---|---|
| `#[job]` | `modo_jobs::JobHandler`, `modo_jobs::JobContext`, `modo_jobs::modo::error::Error`, `modo_jobs::modo::extractor::service::Service`, `modo_jobs::modo_db::extractor::Db`, `modo_jobs::JobQueue`, `modo_jobs::JobId`, `modo_jobs::chrono::{DateTime,Utc}`, `modo_jobs::inventory::submit!`, `modo_jobs::JobRegistration` |

#### modo-upload-macros (parent crate: `modo-upload`) — ALREADY DONE

| Macro | Paths referenced in generated code |
|---|---|
| `FromMultipart` (derive) | `modo_upload::UploadedFile`, `modo_upload::BufferedUpload`, `modo_upload::FromMultipart`, `modo_upload::__internal::{async_trait, axum, mime_matches}`, `modo::HttpError`, `modo::Error`, `modo::validate::validation_error` |

**Note:** `modo-upload` already has an `__internal` module. Its macros mostly use it, but also reference `modo::` paths directly for error types. These `modo::` references are acceptable — they go through the `modo` crate, not `modo-upload`'s own internals.

### Step 1.2: Create `modo::__internal` module

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo/src/lib.rs`
- [x] Add a `#[doc(hidden)] pub mod __internal` inline module that re-exports everything `modo-macros` generated code needs.

```rust
// In modo/src/lib.rs, add after existing re-exports:

/// Internal re-exports for generated code. Not public API — do not rely on these.
#[doc(hidden)]
pub mod __internal {
    // -- handler macro --
    pub use crate::app::AppState;
    pub use crate::router::{MiddlewareFn, ModuleRegistration, RouteRegistration, RouterMiddlewareFn};
    pub use crate::router::Method;

    // -- error_handler macro --
    pub use crate::error::{ErrorHandlerFn, ErrorHandlerRegistration};

    // -- sanitize derive --
    pub use crate::sanitize::{
        Sanitize, SanitizerRegistration, collapse_whitespace, lowercase, normalize_email,
        strip_html_tags, trim, truncate, uppercase,
    };

    // -- validate derive --
    pub use crate::validate::{Validate, is_valid_email, validation_error};
    pub use crate::error::Error;

    // -- main macro --
    pub use crate::app::AppBuilder;
    pub use crate::config::load_or_default;

    // -- view macro (template-gated) --
    #[cfg(feature = "templates")]
    pub use crate::templates::{
        TemplateContext, TemplateEngine, TemplateError, TemplateFunctionEntry,
        TemplateFilterEntry, View, ViewRender,
    };

    // -- third-party re-exports for generated code --
    pub mod axum {
        pub use axum::extract::Path;
        pub use axum::middleware::from_fn;
        pub use axum::response::{IntoResponse, Response};
        pub use axum::routing::{self, MethodRouter};
        pub use axum::Router;
    }
    pub use ::inventory;
    pub use ::serde;
    #[cfg(feature = "templates")]
    pub use ::minijinja;
    pub use ::tokio;
    pub use ::tracing;
    pub use ::tracing_subscriber;
    #[cfg(feature = "static-embed")]
    pub use ::rust_embed;
}
```

### Step 1.3: Create `modo_db::__internal` module

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-db/src/lib.rs`
- [x] Add a `#[doc(hidden)] pub mod __internal` inline module.

```rust
// In modo-db/src/lib.rs, add after existing re-exports:

/// Internal re-exports for generated code. Not public API.
#[doc(hidden)]
pub mod __internal {
    // -- entity macro --
    pub use crate::entity::EntityRegistration;
    pub use crate::error::db_err_to_error;
    pub use crate::helpers::{do_delete, do_insert, do_update};
    pub use crate::hooks::DefaultHooks;
    pub use crate::id::{generate_nanoid, generate_ulid};
    pub use crate::query::{EntityDeleteMany, EntityQuery, EntityUpdateMany};
    pub use crate::record::Record;

    // -- migration macro --
    pub use crate::migration::MigrationRegistration;

    // -- third-party re-exports --
    pub use ::chrono;
    pub use ::inventory;
    pub use ::sea_orm;

    // -- modo error types (used by entity macro for Result<_, modo::Error>) --
    pub use ::modo;
}
```

### Step 1.4: Create `modo_jobs::__internal` module

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-jobs/src/lib.rs`
- [x] Add a `#[doc(hidden)] pub mod __internal` inline module.

```rust
// In modo-jobs/src/lib.rs, add after existing re-exports:

/// Internal re-exports for generated code. Not public API.
#[doc(hidden)]
pub mod __internal {
    pub use crate::handler::{JobContext, JobHandler, JobRegistration};
    pub use crate::queue::JobQueue;
    pub use crate::types::JobId;

    // -- third-party re-exports --
    pub use ::chrono;
    pub use ::inventory;

    // -- cross-crate re-exports --
    pub use ::modo;
    pub use ::modo_db;
}
```

### Step 1.5: Verify `modo_upload::__internal` is complete

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-upload/src/lib.rs`
- [x] The existing `__internal` module already has `mime_matches`, `async_trait`, and `axum`. Verify no additions needed. The `FromMultipart` macro also references `modo::HttpError`, `modo::Error`, and `modo::validate::validation_error` but those go through the `modo` crate directly (the `modo-upload-macros` crate's user always has `modo` as a dependency), so they are fine. **No changes needed for modo-upload.**

### Step 1.6: Update `modo-macros` generated code to use `__internal` paths

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-macros/src/handler.rs`

Replace every `modo::` path in generated code with `modo::__internal::` or `::modo::__internal::`:

| Old path | New path |
|---|---|
| `modo::serde::Deserialize` | `modo::__internal::serde::Deserialize` |
| `modo::serde` (serde crate) | `modo::__internal::serde` |
| `modo::axum::extract::Path` | `modo::__internal::axum::Path` |
| `modo::router::Method::*` | `modo::__internal::Method::*` |
| `modo::axum::routing::get` etc. | `modo::__internal::axum::routing::get` etc. |
| `modo::inventory::submit!` | `modo::__internal::inventory::submit!` |
| `modo::router::RouteRegistration` | `modo::__internal::RouteRegistration` |
| `modo::router::MiddlewareFn` | `modo::__internal::MiddlewareFn` |

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-macros/src/middleware.rs`

| Old path | New path |
|---|---|
| `modo::axum::middleware::from_fn` | `modo::__internal::axum::from_fn` |
| `modo::axum::routing::MethodRouter<modo::app::AppState>` | `modo::__internal::axum::MethodRouter<modo::__internal::AppState>` |
| `modo::axum::Router<modo::app::AppState>` | `modo::__internal::axum::Router<modo::__internal::AppState>` |
| `modo::router::MiddlewareFn` | `modo::__internal::MiddlewareFn` |
| `modo::router::RouterMiddlewareFn` | `modo::__internal::RouterMiddlewareFn` |

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-macros/src/module.rs`

| Old path | New path |
|---|---|
| `modo::router::RouterMiddlewareFn` | `modo::__internal::RouterMiddlewareFn` |
| `modo::inventory::submit!` | `modo::__internal::inventory::submit!` |
| `modo::router::ModuleRegistration` | `modo::__internal::ModuleRegistration` |

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-macros/src/error_handler.rs`

| Old path | New path |
|---|---|
| `modo::inventory::submit!` | `modo::__internal::inventory::submit!` |
| `modo::error::ErrorHandlerRegistration` | `modo::__internal::ErrorHandlerRegistration` |
| `modo::error::ErrorHandlerFn` | `modo::__internal::ErrorHandlerFn` |

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-macros/src/view.rs`

| Old path | New path |
|---|---|
| `::modo::serde::Serialize` | `::modo::__internal::serde::Serialize` |
| `::modo::serde` (crate attr) | `::modo::__internal::serde` |
| `::modo::axum::response::IntoResponse` | `::modo::__internal::axum::IntoResponse` |
| `::modo::axum::response::Response` | `::modo::__internal::axum::Response` |
| `::modo::minijinja::Value` | `::modo::__internal::minijinja::Value` |
| `::modo::templates::View` | `::modo::__internal::View` |
| `::modo::templates::ViewRender` | `::modo::__internal::ViewRender` |
| `::modo::templates::TemplateEngine` | `::modo::__internal::TemplateEngine` |
| `::modo::templates::TemplateContext` | `::modo::__internal::TemplateContext` |
| `::modo::templates::TemplateError` | `::modo::__internal::TemplateError` |

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-macros/src/template_function.rs`

| Old path | New path |
|---|---|
| `::modo::inventory::submit!` | `::modo::__internal::inventory::submit!` |
| `::modo::templates::TemplateFunctionEntry` | `::modo::__internal::TemplateFunctionEntry` |
| `::modo::minijinja::Environment` | `::modo::__internal::minijinja::Environment` |

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-macros/src/template_filter.rs`

| Old path | New path |
|---|---|
| `::modo::inventory::submit!` | `::modo::__internal::inventory::submit!` |
| `::modo::templates::TemplateFilterEntry` | `::modo::__internal::TemplateFilterEntry` |
| `::modo::minijinja::Environment` | `::modo::__internal::minijinja::Environment` |

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-macros/src/main_macro.rs`

| Old path | New path |
|---|---|
| `modo::tokio::runtime::Builder` | `modo::__internal::tokio::runtime::Builder` |
| `modo::tracing_subscriber::EnvFilter` | `modo::__internal::tracing_subscriber::EnvFilter` |
| `modo::tracing_subscriber::fmt` | `modo::__internal::tracing_subscriber::fmt` |
| `modo::app::AppBuilder::new()` | `modo::__internal::AppBuilder::new()` |
| `modo::config::load_or_default` | `modo::__internal::load_or_default` |
| `modo::tracing::error!` | `modo::__internal::tracing::error!` |
| `::modo::rust_embed` | `::modo::__internal::rust_embed` |

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-macros/src/sanitize.rs`

| Old path | New path |
|---|---|
| `modo::sanitize::Sanitize` | `modo::__internal::Sanitize` |
| `modo::sanitize::SanitizerRegistration` | `modo::__internal::SanitizerRegistration` |
| `modo::sanitize::{trim,lowercase,...}` | `modo::__internal::{trim,lowercase,...}` |
| `modo::inventory::submit!` | `modo::__internal::inventory::submit!` |

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-macros/src/validate.rs`

| Old path | New path |
|---|---|
| `modo::validate::Validate` | `modo::__internal::Validate` |
| `modo::validate::validation_error` | `modo::__internal::validation_error` |
| `modo::validate::is_valid_email` | `modo::__internal::is_valid_email` |
| `modo::Error` | `modo::__internal::Error` |

### Step 1.7: Update `modo-db-macros` generated code to use `__internal` paths

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-db-macros/src/entity.rs`

All `modo_db::` path references in quote blocks become `modo_db::__internal::`:

| Old path | New path |
|---|---|
| `modo_db::generate_ulid()` | `modo_db::__internal::generate_ulid()` |
| `modo_db::generate_nanoid()` | `modo_db::__internal::generate_nanoid()` |
| `modo_db::chrono::*` | `modo_db::__internal::chrono::*` |
| `modo_db::sea_orm::*` | `modo_db::__internal::sea_orm::*` |
| `modo_db::inventory::submit!` | `modo_db::__internal::inventory::submit!` |
| `modo_db::EntityRegistration` | `modo_db::__internal::EntityRegistration` |
| `modo_db::db_err_to_error` | `modo_db::__internal::db_err_to_error` |
| `modo_db::Record` | `modo_db::__internal::Record` |
| `modo_db::DefaultHooks` | `modo_db::__internal::DefaultHooks` |
| `modo_db::do_insert` | `modo_db::__internal::do_insert` |
| `modo_db::do_update` | `modo_db::__internal::do_update` |
| `modo_db::do_delete` | `modo_db::__internal::do_delete` |
| `modo_db::EntityQuery` | `modo_db::__internal::EntityQuery` |
| `modo_db::EntityUpdateMany` | `modo_db::__internal::EntityUpdateMany` |
| `modo_db::EntityDeleteMany` | `modo_db::__internal::EntityDeleteMany` |
| `modo::Error` | `modo_db::__internal::modo::Error` |
| `modo::HttpError::NotFound` | `modo_db::__internal::modo::HttpError::NotFound` |

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-db-macros/src/migration.rs`

| Old path | New path |
|---|---|
| `modo_db::inventory::submit!` | `modo_db::__internal::inventory::submit!` |
| `modo_db::MigrationRegistration` | `modo_db::__internal::MigrationRegistration` |

### Step 1.8: Update `modo-jobs-macros` generated code to use `__internal` paths

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-jobs-macros/src/job.rs`

| Old path | New path |
|---|---|
| `modo_jobs::JobHandler` | `modo_jobs::__internal::JobHandler` |
| `modo_jobs::JobContext` | `modo_jobs::__internal::JobContext` |
| `modo_jobs::modo::error::Error` | `modo_jobs::__internal::modo::Error` |
| `modo_jobs::modo::extractor::service::Service` | `modo_jobs::__internal::modo::extractor::service::Service` |
| `modo_jobs::modo_db::extractor::Db` | `modo_jobs::__internal::modo_db::extractor::Db` |
| `modo_jobs::JobQueue` | `modo_jobs::__internal::JobQueue` |
| `modo_jobs::JobId` | `modo_jobs::__internal::JobId` |
| `modo_jobs::chrono::*` | `modo_jobs::__internal::chrono::*` |
| `modo_jobs::inventory::submit!` | `modo_jobs::__internal::inventory::submit!` |
| `modo_jobs::JobRegistration` | `modo_jobs::__internal::JobRegistration` |

### Step 1.9: Keep existing top-level re-exports

- [x] **Important:** Do NOT remove the existing top-level re-exports from `modo/src/lib.rs`, `modo-db/src/lib.rs`, or `modo-jobs/src/lib.rs`. Those are part of the public API (e.g., `pub use axum;`, `pub use sea_orm;`, `pub use inventory;`). The `__internal` module is strictly for proc-macro generated code. The top-level re-exports stay for user code.

### Step 1.10: Build and test

- [x] Run `cargo check` to verify all paths resolve correctly.
- [x] Run `cargo test --workspace` to verify no regressions.
- [x] Run `just check` (fmt + lint + test).
- [x] Build at least one example to confirm handler/module/view macros still work:
  ```bash
  cargo build -p todo-api
  cargo build -p templates
  ```

### Step 1.11: Commit

- [x] Commit with message: `refactor: standardize proc-macro re-exports behind __internal modules (INC-18)`

---

## Item 2: INC-13 — Create shared `ulid_id!` newtype macro

### Step 2.1: Define the `ulid_id!` macro

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo/src/ulid_id.rs` (new file)

```rust
/// Generates a ULID-based newtype ID with standard trait implementations.
///
/// # Usage
///
/// ```rust,ignore
/// modo::ulid_id!(SessionId);
/// modo::ulid_id!(JobId);
/// ```
///
/// Generates:
/// - A newtype struct wrapping `String`
/// - `new()` → generates a new ULID
/// - `from_raw(impl Into<String>)` → wraps existing string without validation
/// - `as_str()` → borrows inner string
/// - `into_string()` → consumes and returns inner string
/// - `Default` (delegates to `new()`)
/// - `Display`, `FromStr` (infallible)
/// - `Serialize`, `Deserialize`
/// - `From<String>`, `From<&str>`, `AsRef<str>`
/// - `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`
#[macro_export]
macro_rules! ulid_id {
    ($name:ident) => {
        /// Unique identifier backed by a ULID string.
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash,
            $crate::serde::Serialize, $crate::serde::Deserialize,
        )]
        pub struct $name(String);

        impl $name {
            /// Generate a new, globally unique ID.
            pub fn new() -> Self {
                Self($crate::ulid::Ulid::new().to_string())
            }

            /// Wrap an existing string as an ID without validation.
            pub fn from_raw(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            /// Borrow the underlying ULID string.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consume the ID, returning the inner `String`.
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl std::str::FromStr for $name {
            type Err = std::convert::Infallible;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(s.to_string()))
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}
```

### Step 2.2: Re-export the macro from `modo/src/lib.rs`

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo/src/lib.rs`
- [x] Add `mod ulid_id;` (before the public API re-exports section, with the other module declarations).
- [x] The `#[macro_export]` attribute automatically makes it available as `modo::ulid_id!`. No additional `pub use` needed.

### Step 2.3: Replace `SessionId` in `modo-session`

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-session/src/types.rs`

**Before:** Lines 1-52 contain a hand-rolled `SessionId` with:
- `#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]`
- `pub struct SessionId(String)`
- `new()` → `ulid::Ulid::new().to_string()`
- `from_raw(impl Into<String>)` → `Self(s.into())`
- `as_str()` → `&self.0`
- `into_string()` → `self.0`
- `Default` → `new()`
- `Display` → `f.write_str(&self.0)`
- `FromStr` → `Ok(Self(s.to_string()))`

**After:** Replace lines 1-52 (the `SessionId` struct and all its impls up to and including the `FromStr` impl) with:

```rust
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::{self, Write};
use std::str::FromStr;

modo::ulid_id!(SessionId);
```

The macro generates everything `SessionId` had, plus `From<String>`, `From<&str>`, and `AsRef<str>` which `SessionId` was previously missing (but `JobId` had). The `from_raw` method is also generated by the macro, so the call site `SessionId::from_raw(&model.id)` in `modo-session/src/store.rs` will continue to work.

**Verify:** The existing tests in `types.rs` (`session_id_generates_unique`, `session_id_ulid_format`, `session_id_display_from_str_roundtrip`, `session_id_from_raw`) should all pass without modification.

### Step 2.4: Replace `JobId` in `modo-jobs`

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-jobs/src/types.rs`

**Before:** Lines 1-56 contain a hand-rolled `JobId` with:
- `#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]`
- `pub struct JobId(String)`
- `new()` → `ulid::Ulid::new().to_string()`
- `as_str()` → `&self.0`
- `into_string()` → `self.0`
- `From<String>`, `From<&str>`, `AsRef<str>`
- `Display`, `FromStr`

**After:** Replace lines 1-56 (everything up to and including the `FromStr` impl) with:

```rust
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

modo::ulid_id!(JobId);
```

**Note:** `JobId` previously derived `Default` (which called `Self(String::default())` producing an empty string). The macro's `Default` calls `new()`, generating a ULID. Check all usages to confirm no code relies on `JobId::default()` producing an empty string.

- [x] Verify no code relies on `JobId::default()` producing an empty string:
  ```bash
  # Search for JobId::default() usage
  grep -rn "JobId::default\|Default.*JobId" modo-jobs/src/ --include="*.rs"
  ```

The macro also adds `from_raw()` which `JobId` didn't have before — this is harmless (additive).

### Step 2.5: Verify `modo-session` depends on `modo`

- [x] Check that `modo-session/Cargo.toml` has `modo` as a dependency. The `ulid_id!` macro uses `$crate::ulid::Ulid` and `$crate::serde::*`, which resolve to `modo::ulid` and `modo::serde` — so this only works when invoked from crates that depend on `modo`.

- [x] **If `modo-session` does NOT depend on `modo`:** Instead of using `modo::ulid_id!`, we have two options:
  1. Add `modo` as a dependency to `modo-session`
  2. Keep `SessionId` hand-rolled

  Check:
  ```bash
  grep 'modo\b' modo-session/Cargo.toml
  ```

  If `modo-session` depends on `modo` already, proceed. If not, add it.

- [x] Similarly verify `modo-jobs` depends on `modo` (it does — confirmed from `modo-jobs/src/lib.rs` which has `pub use modo;`).

### Step 2.6: Update imports in `modo-session` and `modo-jobs`

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-session/src/types.rs`
  - Remove the now-unused `use ulid::Ulid;` import (if present — actually the current code uses `ulid::Ulid::new()` inline).
  - Remove `use std::str::FromStr;` from the top **only if** it's no longer needed by other types in the file. `SessionToken` doesn't use `FromStr`, so it can likely stay removed. But `FromStr` is used by the test `s.parse()`. Since the macro generates the `FromStr` impl, the test just needs `use std::str::FromStr;` in scope — the `use` at file top covers this.
  - Actually, keep the existing `use` imports since `SessionToken` and `SessionData` in the same file need `Serialize`, `Deserialize`, `fmt`, etc. Only the `SessionId`-specific code is replaced.

- [x] **File:** `/Users/dmitrymomot/Dev/modo/modo-jobs/src/types.rs`
  - The remaining `JobState` enum still needs `serde::{Deserialize, Serialize}`, `std::fmt`, and `std::str::FromStr`. Keep those imports.
  - Remove `use ulid::Ulid;` if present (the current code uses `ulid::Ulid::new()` inline, so there's nothing to remove).

### Step 2.7: Build and test

- [x] Run `cargo check` to verify the macro expands correctly.
- [x] Run `cargo test --workspace` to verify `SessionId` and `JobId` tests pass.
- [x] Run `just check` (fmt + lint + test).

### Step 2.8: Commit

- [x] Commit with message: `refactor: replace hand-rolled ULID ID types with ulid_id! macro (INC-13)`

---

## Edge Cases & Risks

1. **Path resolution in user crates:** Generated code uses `modo::__internal::*` which requires the user's crate to have `modo` as a dependency. All examples and typical usage already do — but if someone uses `modo-macros` directly without `modo`, it would break. This is not a supported configuration.

2. **Feature-gated `__internal` items:** Template-related items in `modo::__internal` must be gated with `#[cfg(feature = "templates")]`. The view/template_function/template_filter macros already gate their `inventory::submit!` calls with `#[cfg(feature = "templates")]`, so the `__internal` items are only accessed when the feature is enabled.

3. **`static-embed` feature:** The `rust_embed` re-export in `__internal` must be gated with `#[cfg(feature = "static-embed")]`.

4. **Macro hygiene:** The `ulid_id!` macro uses `$crate::serde` and `$crate::ulid` which resolve correctly when invoked from the `modo` crate. But when invoked from `modo-session` or `modo-jobs` via `modo::ulid_id!`, `$crate` resolves to `modo`, which is correct because `modo` re-exports both `serde` and `ulid`.

5. **`JobId::default()` behavior change:** The old `Default` derived on `JobId` produced `JobId("")` (empty string). The macro's `Default` produces a new ULID. Verify no code depends on the empty-string behavior.

6. **Ordering:** INC-18 should be implemented before INC-13 because the `ulid_id!` macro definition file uses `$crate::serde` and `$crate::ulid`, which are top-level re-exports already present (not `__internal`). The macro itself does not go through `__internal`. So technically they're independent, but doing INC-18 first ensures all the proc-macro path updates are in place before adding the new macro.

## Suggested Test Cases

After both items are complete:

1. `cargo build -p todo-api` — uses `#[handler]`, `#[module]`, `#[entity]` macros
2. `cargo build -p templates` — uses `#[view]`, `#[template_function]` macros
3. `cargo build -p jobs` — uses `#[job]` macro (if example exists)
4. `cargo build -p upload` — uses `FromMultipart` derive
5. `cargo test -p modo-session` — validates `SessionId` via `ulid_id!`
6. `cargo test -p modo-jobs` — validates `JobId` via `ulid_id!`
7. `cargo test --workspace` — full regression check
8. `just check` — fmt + lint + test
