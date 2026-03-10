# Framework Simplification: Crate Mergers

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Reduce modo from 17 crates to 12 by merging csrf, templates, and i18n into the core crate behind disabled-by-default feature flags.

**Architecture:** Move source files from `modo-csrf`, `modo-templates`, `modo-templates-macros`, `modo-i18n`, and `modo-i18n-macros` into `modo` and `modo-macros`. Each merged module lives behind a feature flag (`csrf`, `templates`, `i18n`) that is disabled by default. Downstream crates and examples are updated to use the new paths. Old crate directories and workspace members are removed.

**Tech Stack:** Rust, Cargo workspaces, feature flags, proc macros

**Merge order:** templates first (most depended-on), then csrf, then i18n. Each phase ends with a passing `cargo check` and `just test`.

---

## Phase 1: Merge `modo-templates` + `modo-templates-macros`

### Task 1.1: Copy templates source files into modo

**Files:**
- Create: `modo/src/templates/mod.rs` (from `modo-templates/src/lib.rs`)
- Create: `modo/src/templates/config.rs` (from `modo-templates/src/config.rs`)
- Create: `modo/src/templates/context.rs` (from `modo-templates/src/context.rs`)
- Create: `modo/src/templates/engine.rs` (from `modo-templates/src/engine.rs`)
- Create: `modo/src/templates/error.rs` (from `modo-templates/src/error.rs`)
- Create: `modo/src/templates/middleware.rs` (from `modo-templates/src/middleware.rs`)
- Create: `modo/src/templates/render.rs` (from `modo-templates/src/render.rs`)
- Create: `modo/src/templates/view.rs` (from `modo-templates/src/view.rs`)

**Step 1: Copy source files**

```bash
mkdir -p modo/src/templates
cp modo-templates/src/config.rs modo/src/templates/
cp modo-templates/src/context.rs modo/src/templates/
cp modo-templates/src/engine.rs modo/src/templates/
cp modo-templates/src/error.rs modo/src/templates/
cp modo-templates/src/middleware.rs modo/src/templates/
cp modo-templates/src/render.rs modo/src/templates/
cp modo-templates/src/view.rs modo/src/templates/
```

**Step 2: Create `modo/src/templates/mod.rs`**

Transform `modo-templates/src/lib.rs` into a module file. Remove the crate-level re-exports (`pub use axum`, `pub use serde`, `pub use minijinja`) — those will be handled by `modo/src/lib.rs`. Remove the `pub use modo_templates_macros::view` line — that re-export moves to `modo/src/lib.rs`. Keep only the module declarations and type re-exports.

```rust
pub mod config;
pub mod context;
pub mod engine;
pub mod error;
pub mod middleware;
pub mod render;
pub mod view;

pub use config::TemplateConfig;
pub use context::TemplateContext;
pub use engine::{TemplateEngine, engine};
pub use error::TemplateError;
pub use middleware::ContextLayer;
pub use render::RenderLayer;
pub use view::View;
```

**Step 3: Fix internal references in all copied files**

In every file under `modo/src/templates/`, replace internal crate references:
- `crate::config` → `super::config` (or `crate::templates::config`)
- `crate::context` → `super::context`
- `crate::engine` → `super::engine`
- `crate::error` → `super::error`
- `crate::view` → `super::view`
- `crate::TemplateContext` → `super::TemplateContext`
- `crate::TemplateEngine` → `super::TemplateEngine`
- `crate::View` → `super::View`
- `crate::TemplateError` → `super::TemplateError`

The pattern: any `crate::` that pointed within `modo-templates` now points within the `templates` submodule, so use `super::`.

---

### Task 1.2: Copy view macro into modo-macros

**Files:**
- Create: `modo-macros/src/view.rs` (from `modo-templates-macros/src/lib.rs`)
- Modify: `modo-macros/src/lib.rs`

**Step 1: Copy the view macro implementation**

Copy `modo-templates-macros/src/lib.rs` to `modo-macros/src/view.rs`. Remove the `use proc_macro::TokenStream;` (it will be imported in `lib.rs`). Convert the public `view` function to a module-level `expand` function:

```rust
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, ItemStruct, LitStr, Token};

struct ViewAttr {
    template: LitStr,
    htmx_template: Option<LitStr>,
}

impl Parse for ViewAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let template: LitStr = input.parse()?;
        let mut htmx_template = None;

        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            if key == "htmx" {
                input.parse::<Token![=]>()?;
                htmx_template = Some(input.parse::<LitStr>()?);
            } else {
                return Err(syn::Error::new_spanned(
                    key,
                    "unknown attribute, expected `htmx`",
                ));
            }
        }

        Ok(ViewAttr {
            template,
            htmx_template,
        })
    }
}

pub fn expand(
    attr: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let attr = syn::parse2::<ViewAttr>(attr)?;
    let input = syn::parse2::<ItemStruct>(item)?;

    let struct_name = &input.ident;
    let template_path = &attr.template;

    let view_construction = match &attr.htmx_template {
        Some(htmx_lit) => quote! {
            ::modo::templates::View::new(#template_path, user_context)
                .with_htmx(#htmx_lit)
        },
        None => quote! {
            ::modo::templates::View::new(#template_path, user_context)
        },
    };

    Ok(quote! {
        #[derive(::modo::serde::Serialize)]
        #[serde(crate = "::modo::serde")]
        #input

        impl ::modo::axum::response::IntoResponse for #struct_name {
            fn into_response(self) -> ::modo::axum::response::Response {
                let user_context = ::modo::minijinja::Value::from_serialize(&self);
                let view = #view_construction;
                view.into_response()
            }
        }
    })
}
```

Key changes from original:
- `::modo_templates::View` → `::modo::templates::View`
- `::modo_templates::serde` → `::modo::serde`
- `::modo_templates::axum` → `::modo::axum`
- `::modo_templates::minijinja` → `::modo::minijinja`

**Step 2: Register the view macro in `modo-macros/src/lib.rs`**

Add to `modo-macros/src/lib.rs`:

```rust
mod view;

/// Marks a struct as a view with an associated template.
#[proc_macro_attribute]
pub fn view(attr: TokenStream, item: TokenStream) -> TokenStream {
    view::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
```

---

### Task 1.3: Update modo Cargo.toml and lib.rs

**Files:**
- Modify: `modo/Cargo.toml`
- Modify: `modo/src/lib.rs`

**Step 1: Update `modo/Cargo.toml`**

Add `minijinja` and `futures-util` as optional deps. Remove `modo-templates` and `modo-templates-macros` deps. Update the `templates` feature:

```toml
# Remove these lines:
# modo-templates = { path = "../modo-templates", optional = true }
# modo-templates-macros = { path = "../modo-templates-macros", optional = true }

# Add these optional deps:
minijinja = { version = "2", features = ["loader"], optional = true }
futures-util = { version = "0.3", optional = true }
http = { version = "1", optional = true }

# Update feature:
# templates = ["dep:modo-templates", "dep:modo-templates-macros"]
# becomes:
templates = ["dep:minijinja", "dep:futures-util", "dep:http"]
```

Also check that `tower`, `tower-service`, `tower-layer`, `pin-project-lite` are already deps of modo (needed by templates render/middleware layers). If not, add them as optional deps gated by `templates`.

**Step 2: Update `modo/src/lib.rs`**

```rust
// Remove:
// #[cfg(feature = "templates")]
// pub use modo_templates_macros::view;
// #[cfg(feature = "templates")]
// pub use modo_templates;

// Add:
#[cfg(feature = "templates")]
pub use modo_macros::view;
#[cfg(feature = "templates")]
pub mod templates;
#[cfg(feature = "templates")]
pub use minijinja;
```

**Step 3: Verify compilation**

```bash
cargo check -p modo
cargo check -p modo --features templates
```

Expected: both pass.

---

### Task 1.4: Update downstream crates that depended on `modo-templates`

**Files:**
- Modify: `modo-auth/Cargo.toml` and `modo-auth/src/context_layer.rs`
- Modify: `modo-tenant/Cargo.toml` and `modo-tenant/src/context_layer.rs`
- Modify: `modo-csrf/Cargo.toml` and `modo-csrf/src/middleware.rs` (temporary — will be merged in Phase 2)
- Modify: `modo-i18n/Cargo.toml` and `modo-i18n/src/middleware.rs` (temporary — will be merged in Phase 3)

For each crate:

**Step 1: Update Cargo.toml**

Remove `modo-templates` and `minijinja` optional deps. Update `templates` feature to forward to `modo/templates`:

```toml
# Remove:
# modo-templates = { path = "../modo-templates", optional = true }
# minijinja = { version = "2", optional = true }

# Change feature:
# templates = ["dep:modo-templates", "dep:minijinja"]
# becomes:
templates = ["modo/templates"]
```

**Step 2: Update source references**

Replace `modo_templates::TemplateContext` with `modo::templates::TemplateContext` in:
- `modo-auth/src/context_layer.rs:9`
- `modo-tenant/src/context_layer.rs:11`
- `modo-csrf/src/middleware.rs:81,165`
- `modo-i18n/src/middleware.rs:148`

**Step 3: Update `modo/src/app.rs` references**

Replace all `modo_templates::` with `crate::templates::` in `modo/src/app.rs`:
- Line 408-409: `modo_templates::TemplateEngine` → `crate::templates::TemplateEngine`
- Line 413: `modo_templates::RenderLayer` → `crate::templates::RenderLayer`
- Line 437: `modo_templates::TemplateContext` → `crate::templates::TemplateContext`
- Line 445: `modo_templates::ContextLayer` → `crate::templates::ContextLayer`

---

### Task 1.5: Update templates example

**Files:**
- Modify: `examples/templates/Cargo.toml`
- Modify: `examples/templates/src/main.rs`

**Step 1: Update Cargo.toml**

```toml
# Remove:
# modo-templates = { path = "../../modo-templates" }

# modo dep already has features = ["templates"], keep that
```

**Step 2: Update source**

```rust
// Change:
// use modo_templates::{TemplateConfig, engine};
// To:
use modo::templates::{TemplateConfig, engine};
```

---

### Task 1.6: Copy templates tests into modo

**Files:**
- Create: `modo/tests/templates_render_layer.rs` (from `modo-templates/tests/render_layer.rs`)
- Create: `modo/tests/templates_context_layer.rs` (from `modo-templates/tests/context_layer.rs`)
- Create: `modo/tests/templates_e2e.rs` (from `modo-templates/tests/e2e.rs`)
- Create: `modo/tests/templates_view_macro.rs` (from `modo-templates/tests/view_macro.rs`)
- Copy: templates test fixtures directory if it exists

**Step 1: Copy test files and fixtures**

```bash
cp modo-templates/tests/*.rs modo/tests/
# Rename to avoid conflicts
mv modo/tests/render_layer.rs modo/tests/templates_render_layer.rs
mv modo/tests/context_layer.rs modo/tests/templates_context_layer.rs
mv modo/tests/e2e.rs modo/tests/templates_e2e.rs
mv modo/tests/view_macro.rs modo/tests/templates_view_macro.rs
# Copy test templates if they exist
cp -r modo-templates/tests/templates modo/tests/templates 2>/dev/null || true
```

**Step 2: Update references in test files**

In all copied test files, replace:
- `modo_templates::` → `modo::templates::`
- `#[modo_templates::view(` → `#[modo::view(`

**Step 3: Add `#[cfg(feature = "templates")]` gate to each test file**

Add at the top of each test file:
```rust
#![cfg(feature = "templates")]
```

---

### Task 1.7: Remove old modo-templates crates and verify

**Step 1: Remove workspace members**

Edit `Cargo.toml` (workspace root): remove `"modo-templates"` and `"modo-templates-macros"` from the `members` array.

**Step 2: Remove directories**

```bash
rm -rf modo-templates/
rm -rf modo-templates-macros/
```

**Step 3: Full verification**

```bash
cargo check -p modo
cargo check -p modo --features templates
cargo check -p modo-auth --features templates
cargo check -p modo-tenant --features templates
just test
```

Expected: all pass.

**Step 4: Commit**

```bash
git add -A
git commit -m "refactor: merge modo-templates into modo core behind 'templates' feature flag"
```

---

## Phase 2: Merge `modo-csrf`

### Task 2.1: Copy csrf source files into modo

**Files:**
- Create: `modo/src/csrf/mod.rs` (from `modo-csrf/src/lib.rs`)
- Create: `modo/src/csrf/config.rs` (from `modo-csrf/src/config.rs`)
- Create: `modo/src/csrf/middleware.rs` (from `modo-csrf/src/middleware.rs`)
- Create: `modo/src/csrf/token.rs` (from `modo-csrf/src/token.rs`)
- Create: `modo/src/csrf/template.rs` (from `modo-csrf/src/template.rs`)

**Step 1: Copy source files**

```bash
mkdir -p modo/src/csrf
cp modo-csrf/src/config.rs modo/src/csrf/
cp modo-csrf/src/middleware.rs modo/src/csrf/
cp modo-csrf/src/token.rs modo/src/csrf/
cp modo-csrf/src/template.rs modo/src/csrf/
```

**Step 2: Create `modo/src/csrf/mod.rs`**

Transform `modo-csrf/src/lib.rs` into a module file:

```rust
pub mod config;
pub mod middleware;
pub mod token;

#[cfg(feature = "templates")]
pub mod template;

pub use config::CsrfConfig;
pub use middleware::{CsrfToken, csrf_protection};

#[cfg(feature = "templates")]
pub use template::register_template_functions;
```

Key difference from original: the `CsrfState` trait is **removed**. The middleware will access `AppState` directly instead of going through a trait. Also, the `templates` feature gate now refers to `crate`-level `templates` feature, not a separate `modo-csrf/templates` feature.

**Step 3: Fix internal references**

In `modo/src/csrf/middleware.rs`:
- Replace `crate::config::CsrfConfig` → `super::config::CsrfConfig`
- Replace `crate::token::` → `super::token::`
- Replace the `S: CsrfState` generic parameter with concrete `crate::app::AppState`
- Replace `state.csrf_config()` → direct service lookup: `state.services.get::<super::CsrfConfig>().map(|c| (*c).clone()).unwrap_or_default()`
- Replace `state.csrf_secret()` → `state.server_config.secret_key.as_bytes()`
- Replace `modo_templates::TemplateContext` → `crate::templates::TemplateContext` (behind `#[cfg(feature = "templates")]`)

In `modo/src/csrf/template.rs`:
- Replace `modo_templates::` → `crate::templates::` references

In `modo/src/csrf/token.rs`:
- No changes needed (no external references)

In `modo/src/csrf/config.rs`:
- No changes needed (standalone)

---

### Task 2.2: Remove CsrfState trait shim and update middleware re-exports

**Files:**
- Delete: `modo/src/middleware/csrf.rs` (the 12-line `impl CsrfState for AppState` shim — no longer needed)
- Modify: `modo/src/middleware/mod.rs`

**Step 1: Remove the csrf shim**

Delete `modo/src/middleware/csrf.rs`.

**Step 2: Update middleware/mod.rs**

Change the csrf re-export:

```rust
// Remove:
// #[cfg(feature = "csrf")]
// mod csrf;
// #[cfg(feature = "csrf")]
// pub use modo_csrf::csrf_protection;

// Add:
#[cfg(feature = "csrf")]
pub use crate::csrf::csrf_protection;
```

---

### Task 2.3: Update modo Cargo.toml and lib.rs for csrf

**Files:**
- Modify: `modo/Cargo.toml`
- Modify: `modo/src/lib.rs`

**Step 1: Update `modo/Cargo.toml`**

```toml
# Remove:
# modo-csrf = { path = "../modo-csrf", optional = true }

# Add optional deps:
rand = { version = "0.9", optional = true }
hmac = { version = "0.12", optional = true }
sha2 = { version = "0.10", optional = true }
subtle = { version = "2", optional = true }
form_urlencoded = { version = "1", optional = true }

# Update feature:
# csrf = ["dep:modo-csrf"]
# becomes:
csrf = ["dep:rand", "dep:hmac", "dep:sha2", "dep:subtle", "dep:form_urlencoded"]
```

**Step 2: Update `modo/src/lib.rs`**

```rust
// Remove:
// #[cfg(feature = "csrf")]
// pub use modo_csrf;

// Add:
#[cfg(feature = "csrf")]
pub mod csrf;
```

---

### Task 2.4: Copy csrf tests, remove old crate, verify

**Files:**
- Copy test files from `modo-csrf/` to `modo/tests/`
- Remove: `modo-csrf/` directory
- Modify: `Cargo.toml` (workspace root)

**Step 1: Copy tests**

```bash
cp modo-csrf/tests/*.rs modo/tests/ 2>/dev/null || true
# Prefix with csrf_ and add #![cfg(feature = "csrf")] gate
```

Update test references: `modo_csrf::` → `modo::csrf::`.

**Step 2: Remove workspace member**

Edit workspace `Cargo.toml`: remove `"modo-csrf"` from `members`.

**Step 3: Remove directory**

```bash
rm -rf modo-csrf/
```

**Step 4: Verify**

```bash
cargo check -p modo
cargo check -p modo --features csrf
cargo check -p modo --features "csrf,templates"
just test
```

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: merge modo-csrf into modo core behind 'csrf' feature flag"
```

---

## Phase 3: Merge `modo-i18n` + `modo-i18n-macros`

### Task 3.1: Copy i18n source files into modo

**Files:**
- Create: `modo/src/i18n/mod.rs` (from `modo-i18n/src/lib.rs`)
- Create: `modo/src/i18n/config.rs` (from `modo-i18n/src/config.rs`)
- Create: `modo/src/i18n/entry.rs` (from `modo-i18n/src/entry.rs`)
- Create: `modo/src/i18n/error.rs` (from `modo-i18n/src/error.rs`)
- Create: `modo/src/i18n/extractor.rs` (from `modo-i18n/src/extractor.rs`)
- Create: `modo/src/i18n/locale.rs` (from `modo-i18n/src/locale.rs`)
- Create: `modo/src/i18n/middleware.rs` (from `modo-i18n/src/middleware.rs`)
- Create: `modo/src/i18n/store.rs` (from `modo-i18n/src/store.rs`)
- Create: `modo/src/i18n/template.rs` (from `modo-i18n/src/template.rs`)

**Step 1: Copy source files**

```bash
mkdir -p modo/src/i18n
cp modo-i18n/src/config.rs modo/src/i18n/
cp modo-i18n/src/entry.rs modo/src/i18n/
cp modo-i18n/src/error.rs modo/src/i18n/
cp modo-i18n/src/extractor.rs modo/src/i18n/
cp modo-i18n/src/locale.rs modo/src/i18n/
cp modo-i18n/src/middleware.rs modo/src/i18n/
cp modo-i18n/src/store.rs modo/src/i18n/
cp modo-i18n/src/template.rs modo/src/i18n/
```

**Step 2: Create `modo/src/i18n/mod.rs`**

```rust
pub mod config;
pub mod entry;
pub mod error;
pub mod extractor;
pub mod locale;
pub mod middleware;
pub mod store;

#[cfg(feature = "templates")]
pub mod template;

pub use config::I18nConfig;
pub use entry::Entry;
pub use error::I18nError;
pub use extractor::I18n;
pub use middleware::{layer, layer_with_source};
pub use store::{TranslationStore, load};

#[cfg(feature = "templates")]
pub use template::register_template_functions;
```

**Step 3: Fix internal references**

In all files under `modo/src/i18n/`:
- `crate::config` → `super::config`
- `crate::store` → `super::store`
- `crate::error` → `super::error`
- `crate::entry` → `super::entry`
- `crate::locale` → `super::locale`
- `crate::TranslationStore` → `super::TranslationStore`
- `crate::I18nConfig` → `super::I18nConfig`
- `crate::I18nError` → `super::I18nError`
- `crate::Entry` → `super::Entry`

References to `modo::` types (like `modo::app::AppState`) become `crate::` since i18n is now inside modo:
- `modo::app::AppState` → `crate::app::AppState`
- `modo::Error` → `crate::Error`

References to templates:
- `modo_templates::TemplateContext` → `crate::templates::TemplateContext` (behind `#[cfg(feature = "templates")]`)

---

### Task 3.2: Copy t! macro into modo-macros

**Files:**
- Create: `modo-macros/src/t_macro.rs` (from `modo-i18n-macros/src/lib.rs`)
- Modify: `modo-macros/src/lib.rs`

**Step 1: Copy the t! macro**

Copy `modo-i18n-macros/src/lib.rs` to `modo-macros/src/t_macro.rs`. Convert to module-level `expand` function. The generated code references `i18n.t()` and `i18n.t_plural()` which are methods on the `I18n` extractor — these are called on the user-provided expression so no path changes needed.

```rust
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, LitStr, Token};

struct TInput {
    i18n_expr: Expr,
    key: LitStr,
    vars: Vec<(Ident, Expr)>,
}

impl Parse for TInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let i18n_expr: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let key: LitStr = input.parse()?;

        let mut vars = Vec::new();
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let name: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let value: Expr = input.parse()?;
            vars.push((name, value));
        }

        Ok(TInput {
            i18n_expr,
            key,
            vars,
        })
    }
}

pub fn expand(input: proc_macro2::TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    let input = syn::parse2::<TInput>(input)?;
    let i18n = &input.i18n_expr;
    let key = &input.key;

    let has_count = input.vars.iter().any(|(name, _)| name == "count");

    let var_pairs: Vec<proc_macro2::TokenStream> = input
        .vars
        .iter()
        .map(|(name, value)| {
            let name_str = name.to_string();
            quote! { (#name_str, &(#value).to_string()) }
        })
        .collect();

    if has_count {
        let count_expr = input
            .vars
            .iter()
            .find(|(name, _)| name == "count")
            .map(|(_, expr)| expr)
            .unwrap();

        Ok(quote! {
            #i18n.t_plural(#key, #count_expr as u64, &[#(#var_pairs),*])
        })
    } else {
        Ok(quote! {
            #i18n.t(#key, &[#(#var_pairs),*])
        })
    }
}
```

**Step 2: Register in `modo-macros/src/lib.rs`**

```rust
mod t_macro;

/// Translate a key with optional named variables.
#[proc_macro]
pub fn t(input: TokenStream) -> TokenStream {
    t_macro::expand(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
```

---

### Task 3.3: Update modo Cargo.toml and lib.rs for i18n

**Files:**
- Modify: `modo/Cargo.toml`
- Modify: `modo/src/lib.rs`

**Step 1: Update `modo/Cargo.toml`**

```toml
# Add optional deps needed by i18n:
serde_yaml_ng = { version = "0.10", optional = true }
# axum-extra is already a dep — ensure "cookie" feature is present

# Add feature:
i18n = ["dep:serde_yaml_ng"]
```

Note: `serde`, `axum`, `axum-extra`, `tokio`, `tracing` are already deps of modo.

**Step 2: Update `modo/src/lib.rs`**

```rust
#[cfg(feature = "i18n")]
pub mod i18n;
#[cfg(feature = "i18n")]
pub use modo_macros::t;
```

---

### Task 3.4: Copy i18n tests, remove old crates, verify

**Files:**
- Copy: test files from `modo-i18n/tests/` to `modo/tests/`
- Copy: test translation fixtures if they exist
- Remove: `modo-i18n/`, `modo-i18n-macros/`
- Modify: `Cargo.toml` (workspace root)

**Step 1: Copy tests and fixtures**

```bash
cp modo-i18n/tests/*.rs modo/tests/
# Prefix filenames and add #![cfg(feature = "i18n")] gate
# Update references: modo_i18n:: → modo::i18n::, modo_i18n::t → modo::t
```

**Step 2: Remove workspace members**

Edit workspace `Cargo.toml`: remove `"modo-i18n"` and `"modo-i18n-macros"` from `members`.

**Step 3: Remove directories**

```bash
rm -rf modo-i18n/
rm -rf modo-i18n-macros/
```

**Step 4: Full verification**

```bash
cargo check -p modo
cargo check -p modo --features i18n
cargo check -p modo --features "i18n,templates"
cargo check -p modo --features "csrf,templates,i18n"
just test
```

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: merge modo-i18n into modo core behind 'i18n' feature flag"
```

---

## Phase 4: Final Cleanup

### Task 4.1: Verify no-default-features baseline

**Step 1: Verify modo compiles with zero features**

```bash
cargo check -p modo --no-default-features
```

Expected: pass. Templates, csrf, and i18n code is fully gated.

**Step 2: Verify each feature independently**

```bash
cargo check -p modo --features templates
cargo check -p modo --features csrf
cargo check -p modo --features i18n
cargo check -p modo --features "templates,csrf"
cargo check -p modo --features "templates,i18n"
cargo check -p modo --features "csrf,i18n"
cargo check -p modo --features "templates,csrf,i18n"
```

**Step 3: Verify all downstream crates**

```bash
cargo check -p modo-auth
cargo check -p modo-auth --features templates
cargo check -p modo-tenant
cargo check -p modo-tenant --features templates
cargo check -p modo-session
cargo check -p modo-upload
cargo check -p modo-email
just test
```

---

### Task 4.2: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

Update the architecture section to reflect the new structure:
- `modo/` now contains csrf, templates, i18n behind feature flags
- Remove `modo-csrf`, `modo-templates`, `modo-templates-macros`, `modo-i18n`, `modo-i18n-macros` from the crate list
- Update conventions to use `modo::templates::`, `modo::csrf::`, `modo::i18n::` paths
- Note that all three features are disabled by default

---

### Task 4.3: Final commit and verification

**Step 1: Format and lint**

```bash
just fmt
just check
```

**Step 2: Final commit**

```bash
git add -A
git commit -m "docs: update CLAUDE.md for framework simplification (17 → 12 crates)"
```

---

## Summary of Changes

| Before | After | Feature Flag |
|---|---|---|
| `modo-csrf` (5 files) | `modo/src/csrf/` | `csrf` (disabled by default) |
| `modo-templates` (8 files) | `modo/src/templates/` | `templates` (disabled by default) |
| `modo-templates-macros` (1 file) | `modo-macros/src/view.rs` | (always compiled in macro crate) |
| `modo-i18n` (9 files) | `modo/src/i18n/` | `i18n` (disabled by default) |
| `modo-i18n-macros` (1 file) | `modo-macros/src/t_macro.rs` | (always compiled in macro crate) |

**Workspace members:** 17 → 12
**Crates eliminated:** `modo-csrf`, `modo-templates`, `modo-templates-macros`, `modo-i18n`, `modo-i18n-macros`

## Rollback Strategy

Each phase ends with a commit. If any phase fails, `git reset --hard` to the previous commit.
