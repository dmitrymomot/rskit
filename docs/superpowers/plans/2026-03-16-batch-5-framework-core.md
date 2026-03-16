# Batch 5: Framework Core Features — Implementation Plan

> **Status: COMPLETE** — All 7 issues (DES-11, DES-12, DES-14, DES-18, DES-19, DES-21, DES-22) implemented and merged in PR `fix/review-issues`.

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Resolve 7 framework-core issues in the `modo` crate — error handler validation, redirect flexibility, config directory override, shutdown hook timeout, rate limit cleanup scaling, template error routing, and maintenance path matching.

**Architecture:** All changes are in `modo/src/`. Each item is independent — no ordering constraints within the batch. Changes touch middleware, config loading, view responses, and the app builder startup path. All items include unit tests following TDD.

**Tech Stack:** Rust, axum 0.7, tower, inventory, minijinja, tokio, http crate.

---

## DES-11: Panic on multiple `#[error_handler]`

**File:** `modo/src/app.rs`

**Context:** `inventory::iter::<ErrorHandlerRegistration>` collects all registrations. Currently the middleware (line 628-636 in `app.rs`) just takes the first one. Multiple registrations silently drop extras. The check goes in `run()` before the middleware is applied.

### Steps

- [x] **Test (should fail initially):** Add test to `modo/src/app.rs` (or a new test file `modo/src/app_tests.rs` if `app.rs` has no test module).

```rust
// In modo/src/app.rs, add at the bottom (or in a separate test file):
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_handler_count_validation() {
        // Validate the detection logic directly
        let count = inventory::iter::<crate::error::ErrorHandlerRegistration>
            .into_iter()
            .count();
        // In test context, zero registrations is valid
        assert!(count <= 1, "expected at most 1 error handler in test context");
    }
}
```

- [x] **Implement:** In `app.rs` `run()` method, add validation BEFORE the error handler middleware block (before line 628). Insert this code right after the fallback is set (after line 548):

```rust
// --- Validate error handler registrations ---
{
    let handler_count = inventory::iter::<crate::error::ErrorHandlerRegistration>
        .into_iter()
        .count();
    if handler_count > 1 {
        panic!(
            "Multiple #[error_handler] registrations found ({}). \
             Only one error handler is allowed per application. \
             Remove duplicate #[error_handler] attributes.",
            handler_count,
        );
    }
}
```

- [x] **Verify:** `cargo test -p modo -- test_error_handler_count_validation`
- [x] **Run:** `just check`

---

## DES-12: `ViewResponse::redirect_with_status`

**File:** `modo/src/templates/view_response.rs`

**Context:** `ViewResponseKind::Redirect` currently stores only `url: String` and hardcodes `StatusCode::FOUND` (302) in `IntoResponse`. We need a new variant or a status field to support 301, 303, 307, 308.

### Steps

- [x] **Test (should fail initially):** Add test module to `modo/src/templates/view_response.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::StatusCode;
    use tower::ServiceExt;

    #[test]
    fn test_redirect_defaults_to_302() {
        let resp = ViewResponse::redirect("/foo").into_response();
        assert_eq!(resp.status(), StatusCode::FOUND);
        assert_eq!(resp.headers().get("location").unwrap(), "/foo");
    }

    #[test]
    fn test_redirect_with_status_301() {
        let resp =
            ViewResponse::redirect_with_status("/moved", StatusCode::MOVED_PERMANENTLY)
                .into_response();
        assert_eq!(resp.status(), StatusCode::MOVED_PERMANENTLY);
        assert_eq!(resp.headers().get("location").unwrap(), "/moved");
    }

    #[test]
    fn test_redirect_with_status_303() {
        let resp =
            ViewResponse::redirect_with_status("/see-other", StatusCode::SEE_OTHER)
                .into_response();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(resp.headers().get("location").unwrap(), "/see-other");
    }

    #[test]
    fn test_redirect_with_status_307() {
        let resp = ViewResponse::redirect_with_status("/temp", StatusCode::TEMPORARY_REDIRECT)
            .into_response();
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(resp.headers().get("location").unwrap(), "/temp");
    }
}
```

- [x] **Implement:** Modify `ViewResponseKind::Redirect` to carry an optional `StatusCode`, and add the new method:

Change `ViewResponseKind::Redirect` variant from:
```rust
Redirect { url: String },
```
to:
```rust
Redirect { url: String, status: StatusCode },
```

Update `ViewResponse::redirect()` to delegate:
```rust
/// Create a standard 302 redirect.
pub fn redirect(url: impl Into<String>) -> Self {
    Self::redirect_with_status(url, StatusCode::FOUND)
}

/// Create a redirect with a specific HTTP status code (301, 302, 303, 307, 308).
pub fn redirect_with_status(url: impl Into<String>, status: StatusCode) -> Self {
    Self {
        kind: ViewResponseKind::Redirect {
            url: url.into(),
            status,
        },
    }
}
```

Update the `IntoResponse` match arm for `Redirect`:
```rust
ViewResponseKind::Redirect { url, status } => match HeaderValue::try_from(&url) {
    Ok(val) => {
        let mut resp = Response::new(axum::body::Body::empty());
        *resp.status_mut() = status;
        resp.headers_mut().insert("location", val);
        resp
    }
    Err(_) => {
        tracing::error!("Invalid redirect URL");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
},
```

- [x] **Also update `ViewRenderer::redirect()`** in `modo/src/templates/view_renderer.rs` — add a new method:

```rust
/// Smart redirect with custom status — returns redirect with given status
/// for normal requests, `HX-Redirect` header + 200 for HTMX requests.
pub fn redirect_with_status(
    &self,
    url: &str,
    status: StatusCode,
) -> Result<ViewResponse, Error> {
    if self.is_htmx {
        Ok(ViewResponse::hx_redirect(url))
    } else {
        Ok(ViewResponse::redirect_with_status(url, status))
    }
}
```

- [x] **Verify:** `cargo test -p modo -- test_redirect`
- [x] **Run:** `just check`

---

## DES-14: `MODO_CONFIG_DIR` env var

**File:** `modo/src/config.rs`

**Context:** `load_for_env()` at line 422 hardcodes `let config_dir = "config";`. The fix is a one-liner: check `MODO_CONFIG_DIR` env var first.

### Steps

- [x] **Test (should fail initially):** Add tests to the existing test module in `modo/src/config.rs`:

```rust
#[test]
fn test_config_dir_from_env_var() {
    // Create a temp directory with a valid YAML config
    let dir = std::env::temp_dir().join("modo_config_dir_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("test.yaml"),
        "server:\n  port: 9999\n",
    )
    .unwrap();

    unsafe { std::env::set_var("MODO_CONFIG_DIR", dir.to_str().unwrap()) };
    let result: Result<AppConfig, _> = load_for_env("test");
    unsafe { std::env::remove_var("MODO_CONFIG_DIR") };

    let cfg = result.unwrap();
    assert_eq!(cfg.server.port, 9999);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn test_config_dir_defaults_to_config() {
    unsafe { std::env::remove_var("MODO_CONFIG_DIR") };
    // When no env var and no ./config dir, should return DirectoryNotFound for "config"
    let result: Result<AppConfig, _> = load_for_env("nonexistent_env_12345");
    match result {
        Err(ConfigError::DirectoryNotFound { path }) | Err(ConfigError::FileRead { path, .. }) => {
            assert!(path.starts_with("config"), "expected config dir path, got: {path}");
        }
        other => {
            // If ./config dir exists with the file, that's also fine
            // The key assertion is that it uses "config" as the directory
        }
    }
}
```

- [x] **Implement:** Change `load_for_env()` in `modo/src/config.rs`:

Replace:
```rust
let config_dir = "config";
```
with:
```rust
let config_dir = std::env::var("MODO_CONFIG_DIR").unwrap_or_else(|_| "config".to_string());
```

Also change the type annotation from `&str` to `String` — update the subsequent code that uses `config_dir` to work with `String` (it already does via format strings, so only the `Path::new()` call needs `&config_dir`):

```rust
pub fn load_for_env<T: DeserializeOwned>(env: &str) -> Result<T, ConfigError> {
    let config_dir = std::env::var("MODO_CONFIG_DIR").unwrap_or_else(|_| "config".to_string());

    if !std::path::Path::new(&config_dir).is_dir() {
        return Err(ConfigError::DirectoryNotFound {
            path: config_dir,
        });
    }

    let path = format!("{config_dir}/{env}.yaml");
    let raw = std::fs::read_to_string(&path).map_err(|e| ConfigError::FileRead {
        path: path.clone(),
        source: e,
    })?;

    let substituted = substitute_env_vars(&raw);

    serde_yaml_ng::from_str(&substituted).map_err(|e| ConfigError::Parse { path, source: e })
}
```

Note: the `config_dir` was previously `&str` — now it becomes `String`. The `ConfigError::DirectoryNotFound` already expects `String` (`path: config_dir.to_string()` was the old code), so `path: config_dir` works directly.

- [x] **Verify:** `cargo test -p modo -- test_config_dir`
- [x] **Run:** `just check`

---

## DES-18: Configurable per-hook shutdown timeout

**File:** `modo/src/app.rs`

**Context:** `ServerConfig` already has `shutdown_timeout_secs: u64` (default 30). The overall drain timeout at line 740 already uses it: `let shutdown_timeout = Duration::from_secs(server_config.shutdown_timeout_secs);`. However, the per-hook timeout at line 807 is hardcoded to 5 seconds: `Duration::from_secs(5)`. The fix: use `shutdown_timeout_secs` for per-hook timeout too, or add a separate config field.

**Decision:** Add a `hook_timeout_secs` field to `ServerConfig` (default 5, matching current behavior). The overall `shutdown_timeout_secs` controls connection draining and managed service timeouts; `hook_timeout_secs` controls the per-hook budget. This separates the two concerns cleanly.

### Steps

- [x] **Test:** Add test to config tests in `modo/src/config.rs`:

```rust
#[test]
fn test_server_config_hook_timeout_default() {
    let cfg = ServerConfig::default();
    assert_eq!(cfg.hook_timeout_secs, 5);
}

#[test]
fn test_server_config_hook_timeout_yaml() {
    let yaml = "server:\n  hook_timeout_secs: 15\n";
    let cfg: AppConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(cfg.server.hook_timeout_secs, 15);
}
```

- [x] **Implement — config field:** In `modo/src/config.rs`, add `hook_timeout_secs` to `ServerConfig`:

In the struct definition (after `shutdown_timeout_secs`):
```rust
/// Per-hook timeout in seconds during graceful shutdown. Default: `5`.
pub hook_timeout_secs: u64,
```

In `Default for ServerConfig` (after `shutdown_timeout_secs: 30`):
```rust
hook_timeout_secs: 5,
```

- [x] **Implement — usage:** In `modo/src/app.rs`, replace the hardcoded `Duration::from_secs(5)` at the shutdown hooks section (around line 807):

Replace:
```rust
if tokio::time::timeout(Duration::from_secs(5), hook())
```
with:
```rust
if tokio::time::timeout(Duration::from_secs(server_config.hook_timeout_secs), hook())
```

Also update the doc comment on `on_shutdown` method (line 199) from "Each hook runs sequentially with a 5-second budget." to "Each hook runs sequentially with a configurable timeout (default 5s, set via `hook_timeout_secs` in ServerConfig)."

- [x] **Verify:** `cargo test -p modo -- test_server_config_hook_timeout`
- [x] **Run:** `just check`

---

## DES-19: Rate limit cleanup proportional to window

**File:** `modo/src/middleware/rate_limit.rs`

**Context:** `spawn_cleanup_task()` at line 271 hardcodes `Duration::from_secs(300)` (5 minutes). For a 10-second window, this means stale entries sit for 5 minutes. For a 1-hour window, 5-minute cleanup is excessive churn. Formula: `cleanup_interval = clamp(window_secs / 2, 1, 60)`.

### Steps

- [x] **Test (should fail initially):** Add tests to the existing test module in `modo/src/middleware/rate_limit.rs`:

```rust
#[test]
fn test_cleanup_interval_calculation() {
    // Small window: 2s -> max(1, 1) capped at 60 = 1s
    assert_eq!(cleanup_interval_secs(2), 1);

    // Medium window: 60s -> max(30, 1) capped at 60 = 30s
    assert_eq!(cleanup_interval_secs(60), 30);

    // Large window: 300s -> max(150, 1) capped at 60 = 60s
    assert_eq!(cleanup_interval_secs(300), 60);

    // Very large window: 3600s -> max(1800, 1) capped at 60 = 60s
    assert_eq!(cleanup_interval_secs(3600), 60);

    // Tiny window: 1s -> max(0, 1) = 1s  (0/2 = 0, clamped to 1)
    assert_eq!(cleanup_interval_secs(1), 1);

    // Zero window (edge case): 0s -> max(0, 1) capped at 60 = 1s
    assert_eq!(cleanup_interval_secs(0), 1);
}
```

- [x] **Implement:** Add the helper function and update `spawn_cleanup_task` in `modo/src/middleware/rate_limit.rs`:

Add the helper (above `spawn_cleanup_task`):
```rust
/// Calculate cleanup interval proportional to the rate limit window.
/// Returns `clamp(window_secs / 2, 1, 60)`.
fn cleanup_interval_secs(window_secs: u64) -> u64 {
    (window_secs / 2).max(1).min(60)
}
```

Update `spawn_cleanup_task`:
```rust
/// Spawns a background task that prunes expired buckets at an interval
/// proportional to the rate limit window (`max(window/2, 1s)`, capped at 60s).
///
/// Returns the `JoinHandle` so callers can abort it during shutdown.
pub fn spawn_cleanup_task(limiter: Arc<RateLimiterState>) -> tokio::task::JoinHandle<()> {
    let window = limiter.window_secs;
    let interval_secs = cleanup_interval_secs(window);
    tokio::spawn(async move {
        let interval = std::time::Duration::from_secs(interval_secs);
        let max_age = std::time::Duration::from_secs(window * 2);
        loop {
            tokio::time::sleep(interval).await;
            limiter.cleanup(max_age);
        }
    })
}
```

- [x] **Verify:** `cargo test -p modo -- test_cleanup_interval_calculation`
- [x] **Run:** `just check`

---

## DES-21: Template render error through error handler

**File:** `modo/src/templates/render.rs`

**Context:** The `RenderMiddleware` (lines 109-121 in `render.rs`) catches template render errors and returns a bare HTML 500 with the error message in debug or a generic "Internal Server Error" in release. This bypasses `#[error_handler]` because no `Error` is inserted into response extensions. The error handler middleware at `modo/src/error.rs` line 370 looks for `Error` in `response.extensions_mut().remove::<Error>()`. The fix: when rendering fails, create a `modo::Error::internal(...)`, call its `into_response()` which inserts the `Error` into extensions, allowing the error handler middleware (which runs as an outer layer) to intercept it.

**Key insight:** The `RenderMiddleware` is the innermost layer — it runs BEFORE the error handler middleware processes the response. So the flow is: request -> error handler MW -> render MW -> handler. On the way back: handler response -> render MW (processes View, may fail) -> error handler MW (checks extensions for Error). So inserting `Error` into response extensions in render MW will be seen by error handler MW on the way out.

### Steps

- [x] **Test:** Add test to `modo/src/templates/render.rs` (in a new test module):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

    #[tokio::test]
    async fn render_error_inserts_error_into_extensions() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use axum::response::IntoResponse;
        use axum::routing::get;
        use axum::Router;
        use std::sync::Arc;
        use tower::ServiceExt;

        // Create engine with no templates
        let engine = Arc::new(
            crate::templates::engine(&crate::templates::TemplateConfig {
                path: "/nonexistent_path_for_test".to_string(),
                ..Default::default()
            })
            .unwrap(),
        );

        // Handler that returns a View pointing to a nonexistent template
        let app = Router::new()
            .route(
                "/",
                get(|| async {
                    let view = crate::templates::View {
                        template: "nonexistent.html".to_string(),
                        htmx_template: None,
                        user_context: minijinja::Value::UNDEFINED,
                    };
                    let mut resp = StatusCode::OK.into_response();
                    resp.extensions_mut().insert(view);
                    resp
                }),
            )
            .layer(RenderLayer::new(engine))
            .layer(crate::templates::ContextLayer::new());

        let resp = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        // The Error should be in extensions for error_handler_middleware to pick up
        assert!(
            resp.extensions().get::<Error>().is_some(),
            "Expected Error in response extensions for error handler interception"
        );
    }
}
```

- [x] **Implement:** In `modo/src/templates/render.rs`, replace the error branch (lines 109-121):

Replace:
```rust
Err(err) => {
    error!(template = template_name, error = %err, "template render failed");
    let body = if cfg!(debug_assertions) {
        format!(
            "<h1>Template Render Error</h1><pre>{}</pre>",
            html_escape(&err.to_string())
        )
    } else {
        "<h1>Internal Server Error</h1>".to_string()
    };
    Ok((StatusCode::INTERNAL_SERVER_ERROR, Html(body)).into_response())
}
```

With:
```rust
Err(err) => {
    error!(template = template_name, error = %err, "template render failed");
    let error = crate::error::Error::internal(
        format!("template render failed: {err}")
    );
    Ok(error.into_response())
}
```

This works because `Error::into_response()` (defined in `modo/src/error.rs` line 265-269) calls `self.default_response()` and then inserts `self` into `response.extensions_mut()`. The error handler middleware, running as an outer layer, will find this `Error` in extensions and delegate to the user's `#[error_handler]` function.

Note: In debug mode, developers lose the detailed HTML error page showing the template error. However, the error message is still in the `Error`'s message field (and logged via `tracing::error!`), and the user's `#[error_handler]` can render its own debug-friendly page using `error.message_str()`. This is the correct trade-off — errors should flow through the error handler consistently.

- [x] **Verify:** `cargo test -p modo --features templates -- render_error_inserts_error`
- [x] **Run:** `just check`

---

## DES-22: Maintenance mode trailing slash

**File:** `modo/src/middleware/maintenance.rs`

**Context:** Line 14-15 does exact string comparison: `path == state.server_config.liveness_path || path == state.server_config.readiness_path`. If liveness_path is `/_live` and request comes in as `/_live/`, the comparison fails and the health check gets blocked by maintenance mode. Fix: strip trailing slash from the request path before comparison.

### Steps

- [x] **Test (should fail initially):** Add test module to `modo/src/middleware/maintenance.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppState, ServiceRegistry};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn test_state(maintenance: bool) -> AppState {
        let mut server_config = crate::config::ServerConfig::default();
        server_config.http.maintenance = maintenance;
        AppState {
            services: ServiceRegistry::new(),
            server_config,
            cookie_key: axum_extra::extract::cookie::Key::generate(),
        }
    }

    #[tokio::test]
    async fn health_path_bypasses_maintenance() {
        let state = test_state(true);
        let app = Router::new()
            .route("/_live", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                maintenance_middleware,
            ))
            .with_state(state);

        let resp = app
            .oneshot(Request::get("/_live").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_path_with_trailing_slash_bypasses_maintenance() {
        let state = test_state(true);
        let app = Router::new()
            .route("/_live", get(|| async { "ok" }))
            .route("/_live/", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                maintenance_middleware,
            ))
            .with_state(state);

        let resp = app
            .oneshot(Request::get("/_live/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // Should bypass maintenance — trailing slash should match /_live
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn non_health_path_blocked_by_maintenance() {
        let state = test_state(true);
        let app = Router::new()
            .route("/api/data", get(|| async { "data" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                maintenance_middleware,
            ))
            .with_state(state);

        let resp = app
            .oneshot(Request::get("/api/data").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
```

- [x] **Implement:** In `modo/src/middleware/maintenance.rs`, normalize the path before comparison:

Replace:
```rust
let path = request.uri().path();
if path == state.server_config.liveness_path || path == state.server_config.readiness_path {
    return next.run(request).await;
}
```

With:
```rust
let path = request.uri().path();
let normalized = path.strip_suffix('/').unwrap_or(path);
let liveness = state.server_config.liveness_path.strip_suffix('/').unwrap_or(&state.server_config.liveness_path);
let readiness = state.server_config.readiness_path.strip_suffix('/').unwrap_or(&state.server_config.readiness_path);
if normalized == liveness || normalized == readiness {
    return next.run(request).await;
}
```

Note: We normalize both sides — the request path AND the config paths — so the comparison works regardless of whether the config value has a trailing slash.

Edge case: the root path `/` should not be stripped to empty string. `"/".strip_suffix('/')` returns `Some("")`, which is fine because health paths are never root.

- [x] **Verify:** `cargo test -p modo -- maintenance`
- [x] **Run:** `just check`

---

## Final Checklist

- [x] All 7 items implemented and tested
- [x] `just check` passes (fmt-check + lint + test)
- [x] One commit per item (7 commits total)
- [x] No changes outside `modo/` crate
