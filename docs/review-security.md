# Security Issues

Findings from comprehensive framework review (2026-03-15).

## Severity: High

### ~~SEC-01: CSRF cookie is HttpOnly but double-submit requires JS read~~ [FALSE POSITIVE]

**Location:** `modo/src/csrf/middleware.rs:109`

~~The double-submit cookie pattern where the client must read the cookie to submit it in a header requires the cookie to be readable by JavaScript (i.e., NOT HttpOnly). Setting `http_only(true)` means the header-based CSRF variant can only work in same-page form submissions, not by SPA/fetch-based clients.~~

**Re-review finding:** The middleware supports two submission channels: (1) header-based (`x-csrf-token`) and (2) form body extraction. Neither requires JavaScript to read the cookie. The cookie holds the signed token; the submitted value is the raw token, provided to the page through server-side rendering via the `CsrfToken` extension / template context `csrf_token` variable / form `<input>`. The `HttpOnly` flag correctly prevents JS from stealing the signed cookie. The claim misunderstands the double-submit pattern used here.

---

### ~~SEC-02: CSRF failure bypasses custom error handler~~ [FIXED]

**Location:** `modo/src/csrf/middleware.rs:134,163,169`

CSRF validation returns a bare `StatusCode::FORBIDDEN` without inserting an `Error` extension into response extensions. This means `#[error_handler]` is never invoked for CSRF rejections. The response format is also inconsistent — bare status code body instead of the JSON error format used elsewhere.

**Fix:** All CSRF failure paths now use `HttpError::*.with_message(...)` which produces structured JSON error responses and inserts the `Error` extension for custom error handler interception.

---

### ~~SEC-03: CSRF form body overflow silently returns empty body~~ [FIXED]

**Location:** `modo/src/csrf/middleware.rs:200-202`

When the form body exceeds `max_body_bytes`, the error is swallowed and `Body::empty()` is passed downstream. The downstream handler receives an empty body and may produce confusing validation errors rather than a meaningful "payload too large" response.

**Fix:** `extract_from_form_body` now returns `Result<..., Response>` and oversized bodies produce `HttpError::PayloadTooLarge` (413).

---

### ~~SEC-04: Session token hash leaked via Serde serialization~~ [FIXED]

**Location:** `modo-session/src/types.rs:147-172`

`SessionData` derives `Serialize` with no `#[serde(skip)]` on the `token_hash` field. The `pub(crate)` visibility is a Rust access control, not a Serde control. Any code serializing `SessionData` (e.g., session list API) will include the SHA-256 hash in the output.

**Fix:** Add `#[serde(skip)]` to `SessionData::token_hash`.

---

### ~~SEC-05: Email template variable injection (no HTML escaping)~~ [FIXED]

**Location:** `modo-email/src/template/vars.rs:8`, `modo-email/src/template/layout.rs:115`

Template `{{key}}` substitution inserts values verbatim, and MiniJinja auto-escape is explicitly disabled. User-supplied values (e.g., user names in welcome emails) can inject arbitrary HTML/JS into email bodies.

**Fix:** Added `html_escape()` helper and `substitute_html()` function. Email body substitution now uses `substitute_html()` which HTML-escapes all substituted values. Subject line continues using plain `substitute()` (no escaping needed for RFC 5322 headers). Layout context variables remain unescaped (app-controlled, not user-supplied).

---

### ~~SEC-06: Tenant HeaderResolver spoofable without proxy~~ [FIXED]

**Location:** `modo-tenant/src/resolvers/header.rs:42`

`HeaderResolver` accepts arbitrary header values from clients. Without a reverse proxy that strips/overwrites the tenant header, any client can impersonate any tenant by setting `X-Tenant-Id: victim-tenant`.

**Fix:** Added prominent `# Security` section to `HeaderResolver` struct docstring and a security blockquote in `modo-tenant/README.md` warning that the header is client-controlled and must only be used behind a trusted reverse proxy or for internal/API-only services.

---

### ~~SEC-07: No default HTTP body size limit~~ [FIXED]

**Location:** `modo/src/config.rs` (body_limit field), `modo/src/app.rs`

`body_limit` defaults to `None`, meaning there is no HTTP body size limit by default. Combined with upload's `MultipartForm`, this allows unbounded memory consumption from a single request.

**Fix:** Set a sensible default body limit (e.g., 2MB) in `ServerConfig::default()`.

---

## Severity: Medium

### SEC-08: Upload content type not verified against file bytes

**Location:** `modo-upload/src/validate.rs:63`

The `mime_matches` function compares only the `Content-Type` header from the multipart field. A client can trivially send `Content-Type: image/png` with a PHP or JavaScript payload.

**Fix:** Document this limitation clearly. Consider adding an optional magic-bytes validation step using a crate like `infer` or `tree_magic`.

---

### ~~SEC-09: CORS Mirror + credentials: true allows any origin~~ [FIXED]

**Location:** `modo/src/config.rs`

`CorsOrigins::Mirror` (the default) reflects the request's `Origin` header back. If a user sets `credentials: true` on the default config, any origin can make credentialed cross-origin requests.

**Fix:** Added `CorsConfig::validate()` method that rejects `Mirror` or `Any` origins with `credentials: true`. Called at the top of `into_layer()` — panics on invalid config at startup.

---

### ~~SEC-10: CSRF config validation uses debug_assert (no-op in release)~~ [FIXED]

**Location:** `modo/src/csrf/middleware.rs:40-44`

`debug_assert!` compiles to a no-op in `--release`. Invalid `CsrfConfig` loaded from user YAML (e.g., cookie names with semicolons) silently passes in production.

**Fix:** Replace `debug_assert!` with a `validate()` method called at startup that returns `Result`.

---

### ~~SEC-11: TenantContextLayer fails open on resolver errors~~ [FIXED]

**Location:** `modo-tenant/src/context_layer.rs:107`

When the tenant resolver returns an error (e.g., DB unavailable), the error is logged at WARN level and the request continues without tenant context. Templates that gate content on `{% if tenant %}` will silently render the public/unauthenticated view during infrastructure failures.

**Fix:** Added prominent `# Security: Fail-Open Behavior` doc section to `TenantContextLayer` struct, explaining the fail-open behavior and directing users to the `Tenant<T>` extractor for security-sensitive rendering.

---

### ~~SEC-12: Session fingerprint compared with != (not constant-time)~~ [FALSE POSITIVE]

**Location:** `modo-session/src/middleware.rs:149`

~~The CSRF module uses `subtle::ConstantTimeEq` for token comparison, but session fingerprint comparison uses standard `!=`, which is variable-time. While fingerprint timing attacks are harder to exploit across the network, consistency with the framework's security posture is missing.~~

**Re-review finding:** Session fingerprints are SHA-256 hashes of User-Agent, Accept-Language, and Accept-Encoding headers — they are not secret values. An attacker who controls a request already knows these headers. Constant-time comparison is only meaningful for secret values (tokens, passwords) to prevent timing-based oracle attacks. The fingerprint check detects header-level session hijacking (different device using stolen token). Standard `!=` is appropriate here.

---

### ~~SEC-13: i18n interpolate variable expansion is recursive~~ [FIXED]

**Location:** `modo/src/i18n/interpolate.rs:7`

A user-supplied translation value containing `{admin}` could expand another variable in a later iteration. This is documented in the function docstring but not mitigated.

**Fix:** Rewrote `interpolate()` to use single-pass character-by-character parsing. Substituted values are never re-scanned, preventing recursive variable expansion by construction.

---

### ~~SEC-14: Request ID accepted from client without validation~~ [FIXED]

**Location:** `modo/src/request_id.rs:57-61`

The middleware accepts client-supplied `X-Request-ID` headers verbatim. A client can inject arbitrary values, enabling log injection if request IDs are included in logs without sanitization.

**Re-review note:** `HeaderValue::to_str().ok()` already enforces ASCII-only content (visible ASCII characters only — no null bytes, newlines, or control characters are accepted). The remaining risk is log pollution with arbitrary printable ASCII strings and correlation identifier poisoning, not binary injection.

**Fix:** Added `is_valid_request_id()` validation: max 128 chars, alphanumeric + hyphens/underscores only. Invalid IDs are silently replaced with a server-generated ULID.

---

### ~~SEC-15: IP spoofing when trusted_proxies is empty (default)~~ [FIXED]

**Location:** `modo-session/src/meta.rs:66-83`

When `trusted_proxies` is empty (the default), `extract_client_ip` unconditionally trusts `X-Forwarded-For` and `X-Real-IP` headers. Any client can forge their IP address.

**Fix:** Added prominent `# Security` doc sections to `extract_client_ip` function and `trusted_proxies` field, warning about IP spoofing risk and providing production configuration guidance.

---

### ~~SEC-16: DB error messages may expose internal details~~ [FIXED]

**Location:** `modo-db/src/error.rs:27`

All non-constraint, non-RecordNotFound DB errors are mapped to 500 with the full `DbErr.to_string()`. If `modo::Error::internal` surfaces this in responses, table names, schema info, and query structure could leak to clients.

**Fix:** Catch-all arm now logs the full error via `tracing::error!` for operators but returns generic `"database error"` message to clients.

---

### ~~SEC-17: Layout compile errors silently dropped~~ [FIXED]

**Location:** `modo-email/src/template/layout.rs:74`

`env.add_template_owned(...).ok()` discards any error from loading a custom layout file. A corrupted or syntactically invalid layout is silently ignored, and the template falls back to a missing-template error only at render time.

**Fix:** Added `LayoutEngine::try_new()` that returns `Result` on invalid template syntax. `new()` delegates to `try_new().expect(...)`. Factory uses `try_new()?` so errors propagate at `Mailer` construction time.
