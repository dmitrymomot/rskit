# Batch 3: Logging & Observability — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add structured tracing instrumentation to modo-auth and modo-email, and standardize tracing field naming conventions across the entire workspace.
**Architecture:** Each crate gets targeted `tracing` calls at key decision points (auth resolution, password verification, email rendering/sending) using structured fields. A workspace-wide audit enforces snake_case field naming with no dotted names. The convention is documented in CLAUDE.md.
**Tech Stack:** `tracing` crate (already a transitive dep via `modo`; added explicitly to modo-auth and modo-email `Cargo.toml` for direct use)

---

## INC-04: Add tracing to modo-auth

### Step 1: Add `tracing` dependency to modo-auth

- [ ] Edit `modo-auth/Cargo.toml` — add `tracing` to `[dependencies]`:

**File:** `/Users/dmitrymomot/Dev/modo/modo-auth/Cargo.toml`

```toml
# In [dependencies], add after tokio:
tracing = "0.1"
```

Full `[dependencies]` after edit:
```toml
[dependencies]
modo.workspace = true
modo-session.workspace = true
argon2 = "0.5"
futures-util = "0.3"
serde = { version = "1", features = ["derive"] }
tower = { version = "0.5", features = ["util"] }
tokio = { version = "1", features = ["rt"] }
tracing = "0.1"
```

### Step 2: Add tracing to `extractor.rs` (auth resolution, cache hits/misses)

- [ ] Edit `/Users/dmitrymomot/Dev/modo/modo-auth/src/extractor.rs`

In `resolve_user` function, add tracing for cache hit, user lookup, and auth outcomes.

Replace the entire `resolve_user` function body with:

```rust
async fn resolve_user<U: Clone + Send + Sync + 'static>(
    parts: &mut Parts,
    state: &AppState,
) -> Result<Option<U>, Error> {
    // Fast path: user already resolved by UserContextLayer or a prior extractor
    if let Some(cached) = parts.extensions.get::<ResolvedUser<U>>() {
        tracing::debug!(cache_hit = true, "auth user resolved from extension cache");
        return Ok(Some((*cached.0).clone()));
    }

    let session = SessionManager::from_request_parts(parts, state)
        .await
        .map_err(|_| Error::internal("Auth requires session middleware"))?;

    let user_id = match session.user_id().await {
        Some(id) => id,
        None => {
            tracing::debug!("no session user_id, skipping auth resolution");
            return Ok(None);
        }
    };

    let provider = state
        .services
        .get::<UserProviderService<U>>()
        .ok_or_else(|| {
            Error::internal(format!(
                "UserProviderService<{}> not registered",
                std::any::type_name::<U>()
            ))
        })?;

    let user = provider.find_by_id(&user_id).await?;

    if let Some(ref u) = user {
        tracing::debug!(user_id = %user_id, cache_hit = false, "auth user resolved from provider");
        parts.extensions.insert(ResolvedUser(Arc::new(u.clone())));
    } else {
        tracing::warn!(user_id = %user_id, "session references non-existent user");
    }

    Ok(user)
}
```

The `Auth<U>` and `OptionalAuth<U>` `FromRequestParts` impls remain unchanged (the tracing happens inside `resolve_user`).

### Step 3: Add tracing to `password.rs` (password hashing and verification timing)

- [ ] Edit `/Users/dmitrymomot/Dev/modo/modo-auth/src/password.rs`

Add `use std::time::Instant;` at the top (after the existing imports).

In `hash_password`, wrap the spawn_blocking with timing:

```rust
    pub async fn hash_password(&self, password: &str) -> Result<String, modo::Error> {
        let params = self.params.clone();
        let password = password.to_owned();

        let start = std::time::Instant::now();
        let result = tokio::task::spawn_blocking(move || {
            let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
            let salt = SaltString::generate(&mut OsRng);

            argon2
                .hash_password(password.as_bytes(), &salt)
                .map(|h| h.to_string())
                .map_err(|e| modo::Error::internal(format!("password hashing failed: {e}")))
        })
        .await
        .map_err(|e| modo::Error::internal(format!("password hashing task failed: {e}")))?;

        tracing::debug!(duration_ms = %start.elapsed().as_millis(), "password hash completed");
        result
    }
```

In `verify_password`, wrap with timing and log the outcome:

```rust
    pub async fn verify_password(&self, password: &str, hash: &str) -> Result<bool, modo::Error> {
        let params = self.params.clone();
        let password = password.to_owned();
        let hash = hash.to_owned();

        let start = std::time::Instant::now();
        let result = tokio::task::spawn_blocking(move || {
            let parsed = PasswordHash::new(&hash)
                .map_err(|e| modo::Error::internal(format!("invalid password hash: {e}")))?;

            match Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
                .verify_password(password.as_bytes(), &parsed)
            {
                Ok(()) => Ok(true),
                Err(argon2::password_hash::Error::Password) => Ok(false),
                Err(e) => Err(modo::Error::internal(format!(
                    "password verification failed: {e}"
                ))),
            }
        })
        .await
        .map_err(|e| modo::Error::internal(format!("password verification task failed: {e}")))?;

        let elapsed_ms = start.elapsed().as_millis();
        match &result {
            Ok(true) => tracing::debug!(duration_ms = %elapsed_ms, "password verification succeeded"),
            Ok(false) => tracing::debug!(duration_ms = %elapsed_ms, "password verification failed (mismatch)"),
            Err(_) => {} // error already in the Result
        }

        result
    }
```

### Step 4: Add tracing to `context_layer.rs` (user context injection)

- [ ] Edit `/Users/dmitrymomot/Dev/modo/modo-auth/src/context_layer.rs`

Inside the `call` method of `UserContextMiddleware`, add tracing around the user resolution:

Replace the async block body (lines 99-116) with:

```rust
        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            // Try to get user_id from session extensions
            let user_id = modo_session::user_id_from_extensions(&parts.extensions);

            if let Some(user_id) = user_id
                && let Ok(Some(user)) = user_svc.find_by_id(&user_id).await
            {
                tracing::debug!(user_id = %user_id, "injected user into template context");
                if let Some(ctx) = parts.extensions.get_mut::<TemplateContext>() {
                    ctx.insert("user", modo::minijinja::Value::from_serialize(&user));
                }
                parts.extensions.insert(ResolvedUser(Arc::new(user)));
            } else if let Some(ref user_id) = user_id {
                tracing::debug!(user_id = %user_id, "user context layer: user not found for session user_id");
            }

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
```

### Step 5: Verify INC-04

- [ ] Run: `cargo check -p modo-auth --all-features`
- [ ] Run: `cargo test -p modo-auth`
- [ ] Run: `cargo test -p modo-auth --features templates`

---

## INC-05: Add tracing to modo-email

### Step 1: Add `tracing` dependency to modo-email

- [ ] Edit `modo-email/Cargo.toml` — add `tracing` to `[dependencies]`:

**File:** `/Users/dmitrymomot/Dev/modo/modo-email/Cargo.toml`

```toml
# In [dependencies], add after minijinja:
tracing = "0.1"
```

Full `[dependencies]` after edit:
```toml
[dependencies]
modo.workspace = true
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml_ng = "0.10"
async-trait = "0.1"
pulldown-cmark = "0.12"
minijinja = { version = "2", features = ["loader"] }
tracing = "0.1"
lettre = { version = "0.11", features = ["tokio1-native-tls", "builder", "smtp-transport"], optional = true }
reqwest = { version = "0.12", features = ["json"], optional = true }
```

### Step 2: Add tracing to `mailer.rs` (render and send)

- [ ] Edit `/Users/dmitrymomot/Dev/modo/modo-email/src/mailer.rs`

In the `render` method, add tracing for template resolution and layout rendering:

```rust
    pub fn render(&self, email: &SendEmail) -> Result<MailMessage, modo::Error> {
        let locale = email.locale.as_deref().unwrap_or("");
        let template_name = &email.template;

        tracing::debug!(
            template_name = %template_name,
            locale = %locale,
            "resolving email template"
        );

        let template = self.templates.get(template_name, locale)?;

        // Substitute variables in subject and body.
        let subject = vars::substitute(&template.subject, &email.context);
        let body = vars::substitute_html(&template.body, &email.context);

        // Validate brand_color as a CSS hex color; fall back to default if invalid.
        let button_color = email
            .context
            .get("brand_color")
            .and_then(|v| v.as_str())
            .filter(|s| is_valid_hex_color(s))
            .unwrap_or(markdown::DEFAULT_BUTTON_COLOR);

        // Render Markdown body to HTML and plain text in one pass.
        let (html_body, text) = markdown::render(&body, button_color);

        // Wrap HTML body in a layout.
        let layout_name = template.layout.as_deref().unwrap_or("default");

        tracing::debug!(
            layout_name = %layout_name,
            template_name = %template_name,
            "rendering email layout"
        );

        let mut layout_map: std::collections::BTreeMap<String, minijinja::Value> = email
            .context
            .iter()
            .map(|(k, v)| (k.clone(), minijinja::Value::from_serialize(v)))
            .collect();
        layout_map.insert("content".to_string(), minijinja::Value::from(html_body));
        layout_map.insert(
            "subject".to_string(),
            minijinja::Value::from(subject.as_str()),
        );
        let layout_ctx = minijinja::Value::from_serialize(&layout_map);
        let html = self.layout_engine.render(layout_name, &layout_ctx)?;

        // Resolve sender (per-email override or default).
        let sender = email.sender.as_ref().unwrap_or(&self.default_sender);

        Ok(MailMessage {
            from: sender.format_address(),
            reply_to: sender.reply_to.clone(),
            to: email.to.clone(),
            subject,
            html,
            text,
        })
    }
```

In the `send` method, add tracing for send attempts and failures:

```rust
    pub async fn send(&self, email: &SendEmail) -> Result<(), modo::Error> {
        let to = email.to.join(", ");
        let template_name = &email.template;

        tracing::info!(
            to = %to,
            template_name = %template_name,
            "sending email"
        );

        let message = self.render(email)?;

        if let Err(e) = self.transport.send(&message).await {
            tracing::error!(
                to = %to,
                template_name = %template_name,
                error = %e,
                "email send failed"
            );
            return Err(e);
        }

        tracing::info!(
            to = %to,
            template_name = %template_name,
            "email sent successfully"
        );

        Ok(())
    }
```

### Step 3: Add tracing to `template/filesystem.rs` (template resolution)

- [ ] Edit `/Users/dmitrymomot/Dev/modo/modo-email/src/template/filesystem.rs`

In the `TemplateProvider::get` implementation for `FilesystemProvider`, add debug tracing:

```rust
impl TemplateProvider for FilesystemProvider {
    fn get(&self, name: &str, locale: &str) -> Result<EmailTemplate, modo::Error> {
        let path = self
            .resolve_path(name, locale)
            .ok_or_else(|| {
                tracing::debug!(
                    template_name = %name,
                    locale = %locale,
                    "email template not found on filesystem"
                );
                modo::Error::internal(format!("Email template not found: {name}"))
            })?;

        tracing::debug!(
            template_name = %name,
            locale = %locale,
            path = %path.display(),
            "loading email template from filesystem"
        );

        let raw = std::fs::read_to_string(&path).map_err(|e| {
            modo::Error::internal(format!("Failed to read template {}: {e}", path.display()))
        })?;

        EmailTemplate::parse(&raw)
    }
}
```

### Step 4: Add tracing to `template/layout.rs` (layout rendering)

- [ ] Edit `/Users/dmitrymomot/Dev/modo/modo-email/src/template/layout.rs`

In the `render` method of `LayoutEngine`, add debug tracing:

```rust
    pub fn render(
        &self,
        layout_name: &str,
        context: &minijinja::Value,
    ) -> Result<String, modo::Error> {
        let template_name = format!("layouts/{layout_name}.html");

        let tmpl = self
            .env
            .get_template(&template_name)
            .map_err(|_| {
                tracing::debug!(layout_name = %layout_name, "email layout not found");
                modo::Error::internal(format!("Layout not found: {layout_name}"))
            })?;

        tmpl.render(context)
            .map_err(|e| {
                tracing::error!(layout_name = %layout_name, error = %e, "email layout render failed");
                modo::Error::internal(format!("Layout render error: {e}"))
            })
    }
```

### Step 5: Add tracing to transport implementations

- [ ] Edit `/Users/dmitrymomot/Dev/modo/modo-email/src/transport/smtp.rs`

In the `MailTransport::send` impl for `SmtpTransport`, add tracing around the send call. Replace the final `self.mailer.send(email)...` block:

```rust
        tracing::debug!(to = ?message.to, subject = %message.subject, "sending email via SMTP");

        self.mailer
            .send(email)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "SMTP send failed");
                modo::Error::internal(format!("SMTP send failed: {e}"))
            })?;

        Ok(())
```

- [ ] Edit `/Users/dmitrymomot/Dev/modo/modo-email/src/transport/resend.rs`

In the `MailTransport::send` impl for `ResendTransport`, add tracing. Replace the response check block:

```rust
        tracing::debug!(to = ?message.to, subject = %message.subject, "sending email via Resend API");

        let resp = self
            .client
            .post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Resend request failed");
                modo::Error::internal(format!("Resend request failed: {e}"))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            tracing::error!(status = %status, body = %text, "Resend API error");
            return Err(modo::Error::internal(format!(
                "Resend API error ({status}): {text}"
            )));
        }

        Ok(())
```

### Step 6: Verify INC-05

- [ ] Run: `cargo check -p modo-email --all-features`
- [ ] Run: `cargo test -p modo-email`
- [ ] Run: `cargo test -p modo-email --features resend`

---

## INC-07: Standardize structured tracing fields

### Full Audit of Existing Tracing Calls

Below is the complete inventory of every structured tracing field across the workspace, organized by file. Fields marked **INCONSISTENT** need renaming. Fields marked **OK** already follow snake_case convention.

#### File: `modo/src/middleware/catch_panic.rs:18`
```rust
tracing::error!(panic.message = %msg, "Handler panicked");
```
- `panic.message` — **INCONSISTENT** (dotted name). Change to `panic_message`.

#### File: `modo/src/cookies/manager.rs:196`
```rust
tracing::debug!(cookie = name, error = %e, "failed to deserialize cookie JSON");
```
- `cookie` — **OK** (snake_case)
- `error` — **OK**

#### File: `modo/src/cookies/manager.rs:218`
```rust
tracing::warn!(max_age, "cookie max_age exceeds i64::MAX, clamping");
```
- `max_age` — **OK**

#### File: `modo/src/error.rs:219-224`
```rust
tracing::error!(
    status = status.as_u16(),
    code = %self.code,
    message = %self.message,
    source = ?self.source,
    "Server error"
);
```
- All fields — **OK** (snake_case)

#### File: `modo/src/health.rs:31`
```rust
tracing::error!(error = %e, "Readiness check failed");
```
- `error` — **OK**

#### File: `modo/src/logging.rs:31-38`
```rust
tracing::span!($level, "http_request",
    method = %request.method(),
    uri = %request.uri(),
    version = ?request.version(),
    request_id = %request_id,
)
```
- All fields — **OK** (snake_case)

#### File: `modo/src/sse/broadcast.rs:225`
```rust
tracing::warn!(skipped = n, "SSE subscriber lagged, skipping {n} messages");
```
- `skipped` — **OK**

#### File: `modo/src/sse/sender.rs:104`
```rust
tracing::debug!(error = %e, "SSE channel closure ended with error")
```
- `error` — **OK**

#### File: `modo/src/csrf/middleware.rs:42`
```rust
tracing::error!(error = %e, "Invalid CsrfConfig — rejecting request");
```
- `error` — **OK**

#### File: `modo-session/src/middleware.rs:150-153`
```rust
tracing::warn!(
    session_id = session.id.as_str(),
    user_id = session.user_id,
    "Session fingerprint mismatch..."
);
```
- All fields — **OK**

#### File: `modo-session/src/middleware.rs:205-207`
```rust
tracing::error!(
    session_id = session.id.as_str(),
    "Failed to touch session: {e}"
);
```
- `session_id` — **OK**

#### File: `modo-session/src/middleware.rs:286-288`
```rust
tracing::warn!(
    cookie_name = name,
    "Failed to serialize session cookie: {e}"
);
```
- `cookie_name` — **OK**

#### File: `modo-session/src/manager.rs:59-61`
```rust
tracing::error!(
    session_id = session.id.as_str(),
    "Failed to destroy previous session..."
);
```
- `session_id` — **OK**

#### File: `modo-session/src/manager.rs:208`
```rust
tracing::warn!(key, error = %e, "Failed to deserialize session data key");
```
- `key` — **OK**
- `error` — **OK**

#### File: `modo-session/src/cleanup.rs:19`
```rust
tracing::info!(count, "purged expired sessions");
```
- `count` — **OK**

#### File: `modo-db/src/pool.rs:35`
No structured fields (just message string) — **OK**

#### File: `modo-db/src/error.rs:28`
```rust
tracing::error!(error = %e, "database error");
```
- `error` — **OK**

#### File: `modo-db/src/connect.rs:24`
```rust
info!(url = %redact_url(&config.url), "Database connected");
```
- `url` — **OK**

#### File: `modo-db/src/sync.rs:74-78`
```rust
tracing::error!(
    table = reg.table_name,
    sql = sql,
    error = %e,
    "Failed to execute extra SQL for entity"
);
```
- All fields — **OK**

#### File: `modo-upload/src/extractor.rs:65-68`
```rust
modo::tracing::warn!(
    size = %s,
    error = %e,
    "failed to parse max_file_size from UploadConfig, ignoring limit"
);
```
- `size` — **OK**
- `error` — **OK**

#### File: `modo-jobs/src/runner.rs:236`
```rust
info!("Job runner started (worker_id={worker_id})");
```
- No structured fields — **INCONSISTENT** (worker_id in message string instead of structured field). Change to:
```rust
info!(worker_id = %worker_id, "Job runner started");
```

#### File: `modo-jobs/src/runner.rs:254`
```rust
info!(queue = %ctx.queue_name, "Poll loop shutting down");
```
- `queue` — **OK**

#### File: `modo-jobs/src/runner.rs:291`
```rust
error!(queue = %ctx.queue_name, error = %e, "Failed to claim job");
```
- All fields — **OK**

#### File: `modo-jobs/src/runner.rs:374`
```rust
error!(job_id = %job.id, job_name = %job_name, "No handler registered for job");
```
- All fields — **OK**

#### File: `modo-jobs/src/runner.rs:396-399`
```rust
error!(
    job_id = %job.id, job_name = %job_name, queue = %queue,
    attempt = job.attempts, max_attempts = job.max_attempts,
    error = %e, "Job failed"
);
```
- All fields — **OK**

#### File: `modo-jobs/src/runner.rs:404-407`
```rust
error!(
    job_id = %job.id, job_name = %job_name, queue = %queue,
    attempt = job.attempts, max_attempts = job.max_attempts,
    "Job timed out"
);
```
- All fields — **OK**

#### File: `modo-jobs/src/runner.rs:459`
```rust
error!(job_id = id, error = %e, "Failed to mark job completed");
```
- All fields — **OK**

#### File: `modo-jobs/src/runner.rs:493`
```rust
error!(job_id = &job.id, error = %e, "Failed to schedule job retry");
```
- All fields — **OK**

#### File: `modo-jobs/src/runner.rs:515`
```rust
error!(job_id = id, error = %e, "Failed to mark job dead");
```
- All fields — **OK**

#### File: `modo-jobs/src/runner.rs:550`
```rust
warn!(count = result.rows_affected, "Reaped stale jobs");
```
- `count` — **OK**

#### File: `modo-jobs/src/runner.rs:554`
```rust
error!(error = %e, "Failed to reap stale jobs");
```
- `error` — **OK**

#### File: `modo-jobs/src/runner.rs:592`
```rust
info!(count = result.rows_affected, "Cleaned up old jobs");
```
- `count` — **OK**

#### File: `modo-jobs/src/cron.rs:39`
```rust
info!(job = reg.name, cron = cron_expr, "Scheduled cron job");
```
- `job` — **OK** (but note: other places use `job_name`). Acceptable since this is a different context (cron scheduling).
- `cron` — **OK**

#### File: `modo-jobs/src/cron.rs:64`
```rust
info!(job = name, "Cron schedule exhausted, stopping");
```
- `job` — **OK**

#### File: `modo-jobs/src/cron.rs:96`
```rust
info!(job = name, "Cron job completed");
```
- `job` — **OK**

#### File: `modo-jobs/src/cron.rs:105`
```rust
error!(job = name, error = %err_msg, "Cron job failed");
```
- All fields — **OK**

#### File: `modo-jobs/src/cron.rs:107-110`
```rust
warn!(
    job = name,
    consecutive_failures,
    "Cron job has failed..."
);
```
- All fields — **OK**

#### File: `modo-tenant/src/context_layer.rs:120`
```rust
tracing::warn!("TenantContextLayer: tenant resolution failed: {e}");
```
- No structured fields — **INCONSISTENT** (error in message string instead of structured field). Change to:
```rust
tracing::warn!(error = %e, "TenantContextLayer: tenant resolution failed");
```

### Summary of Changes Needed

| # | File | Line | Field | Issue | Fix |
|---|------|------|-------|-------|-----|
| 1 | `modo/src/middleware/catch_panic.rs` | 18 | `panic.message` | Dotted name | `panic_message` |
| 2 | `modo-jobs/src/runner.rs` | 236 | (none) | `worker_id` in message string | Add as structured field |
| 3 | `modo-tenant/src/context_layer.rs` | 120 | (none) | `{e}` in message string | Add `error = %e` structured field |

### Step 1: Fix `panic.message` dotted field

- [ ] Edit `/Users/dmitrymomot/Dev/modo/modo/src/middleware/catch_panic.rs`

**Before:**
```rust
        tracing::error!(panic.message = %msg, "Handler panicked");
```

**After:**
```rust
        tracing::error!(panic_message = %msg, "Handler panicked");
```

### Step 2: Fix unstructured `worker_id` in jobs runner

- [ ] Edit `/Users/dmitrymomot/Dev/modo/modo-jobs/src/runner.rs`

**Before (line 236):**
```rust
    info!("Job runner started (worker_id={worker_id})");
```

**After:**
```rust
    info!(worker_id = %worker_id, "Job runner started");
```

### Step 3: Fix unstructured error in tenant context layer

- [ ] Edit `/Users/dmitrymomot/Dev/modo/modo-tenant/src/context_layer.rs`

**Before (line 120):**
```rust
                    tracing::warn!("TenantContextLayer: tenant resolution failed: {e}");
```

**After:**
```rust
                    tracing::warn!(error = %e, "TenantContextLayer: tenant resolution failed");
```

### Step 4: Document tracing field naming convention in CLAUDE.md

- [ ] Edit `/Users/dmitrymomot/Dev/modo/CLAUDE.md`

Add the following entry at the end of the `## Conventions` section (after the `modo-db update` line):

```markdown
- Tracing fields: always snake_case (`user_id`, `session_id`, `job_id`) — never dotted names (`panic.message`) which require string literal syntax and can break subscribers
```

### Step 5: Verify INC-07

- [ ] Run: `just check`

---

## Commit Strategy

One commit per item, in order:

1. **INC-04:** `feat(modo-auth): add tracing instrumentation for auth resolution and password hashing`
2. **INC-05:** `feat(modo-email): add tracing instrumentation for email rendering and sending`
3. **INC-07:** `refactor: standardize tracing field naming to snake_case across workspace`
