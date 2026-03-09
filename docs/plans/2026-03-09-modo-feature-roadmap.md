# modo Feature Roadmap

Prioritized feature list for the modo framework, grouped by impact for micro-SaaS use cases.

## Tier 1 — High Impact

### 1. `modo-csrf` — CSRF Protection

Double-submit cookie pattern. Required for any form-based app.

- Generate CSRF token, store in signed cookie
- Middleware validates token on POST/PUT/PATCH/DELETE
- Template helper to inject hidden field
- `#[middleware(csrf_protection)]` on handlers/modules
- Integration with `modo-templates` (auto-inject token into context)

### 2. `modo-mail` — Email Sending

Email is table stakes for SaaS — signup confirmation, password reset, notifications.

- `MailTransport` trait with pluggable backends
- SMTP backend (lettre)
- HTTP API backends (Resend, AWS SES)
- `MailMessage` builder: to, from, subject, body (text + HTML)
- Integration with `modo-templates` for HTML email rendering
- Integration with `modo-i18n` for localized emails
- `Mail` extractor for handlers
- Async sending (direct or via `modo-jobs` queue)

### 3. `modo-cache` — Caching Layer

In-memory and optional Redis support for performance-critical paths.

- `CacheStore` trait with pluggable backends
- In-memory backend (moka — async, TTL, max size)
- Redis backend (optional feature flag)
- `Cache<T>` extractor for handlers
- Key-value API: `get`, `set`, `delete`, `exists`, `get_or_set`
- TTL per entry
- Cache invalidation helpers
- Use cases: session caching, rate limit state, query result caching

### 5. Static File Serving — in `modo` core or `modo-static`

Serve CSS, JS, images, fonts.

- Dev mode: serve from filesystem directory (live reload friendly)
- Prod mode: embedded via `rust-embed` (single binary)
- Configurable path prefix (e.g., `/static/`, `/assets/`)
- Cache headers (ETag, Cache-Control)
- MIME type detection
- Gzip/Brotli compression (pre-compressed or on-the-fly)

## Tier 2 — Strong Value-Add

### 6. `modo-sse` — Server-Sent Events

Real-time server-to-client streaming. Simpler than WebSockets, works with HTMX.

- `SseStream` response type for handlers
- Named events, retry intervals, last-event-id
- Channel-based broadcasting (one-to-many)
- Integration with HTMX `hx-ext="sse"`
- Automatic client reconnection support
- Use cases: job progress, live notifications, dashboard updates

### 7. Pagination Helpers — in `modo-db`

Every API and list view needs pagination.

- `Paginated<T>` response wrapper
- Offset-based pagination (page + per_page)
- Cursor-based pagination (for stable ordering)
- `PaginationParams` extractor (from query string)
- Response metadata: total count, page count, next/prev cursors
- Works with SeaORM `Select` queries

### 8. API Key Authentication — in `modo-auth`

Many SaaS products expose APIs to customers.

- `ApiKey` extractor for handlers
- Key generation with prefix (e.g., `modo_live_xxx`, `modo_test_xxx`)
- Hashed storage (SHA256, never store plaintext)
- Key scoping (read, write, admin)
- Per-key rate limiting integration
- Key rotation (create new before revoking old)
- `ApiKeyProvider` trait (similar to `UserProvider`)

### 10. Audit Logging — `modo-audit`

Who did what when. Compliance requirement for B2B SaaS.

- `AuditLog::record(actor, action, resource, details)`
- Database-backed (auto-registered entity)
- Actor: user ID, API key, system
- Action: create, update, delete, login, etc.
- Resource: entity type + ID
- Optional diff/details JSON
- `AuditQuery` for filtering/searching logs
- Middleware option for automatic CRUD logging

## Tier 3 — Nice to Have

### 11. WebSocket Support — `modo-ws`

Full-duplex communication for interactive features.

- `WebSocket` extractor upgrade
- Message types: text, binary, ping/pong
- Room/channel abstraction
- Authentication on upgrade
- Use cases: chat, collaborative editing, live cursors

### 12. CLI Scaffolding Tool

Developer experience improvement for project bootstrapping.

- `modo new <project>` — scaffold new project
- `modo generate handler <name>` — create handler file
- `modo generate entity <name>` — create entity with migration
- `modo generate job <name>` — create background job
- Template-based generation

### 13. OpenAPI Generation

Auto-generate API documentation from handler signatures.

- Extract routes, methods, path params from handler macros
- Request/response schema from Serialize/Deserialize types
- Serve Swagger UI at `/docs`
- Export OpenAPI 3.1 JSON/YAML
- Integration with validation rules for schema constraints

### 14. Metrics & Observability — `modo-metrics`

Production monitoring and alerting.

- Prometheus metrics endpoint (`/metrics`)
- Request metrics: latency, status codes, in-flight count
- Database metrics: query count, connection pool stats
- Job metrics: queue depth, processing time, failure rate
- Custom application metrics API
- Optional Grafana dashboard templates

### 15. Webhook Delivery — `modo-webhooks`

Outbound webhooks for SaaS integrations.

- Webhook endpoint registration (URL, events, secret)
- HMAC-SHA256 request signing
- Delivery with retry (exponential backoff via `modo-jobs`)
- Delivery log with status tracking
- Event filtering per endpoint
- Payload serialization (JSON)

### 16. RBAC / Permissions

Role-based access control for multi-user apps.

- `#[middleware(require_role("admin"))]`
- `#[middleware(require_permission("posts:write"))]`
- `Permissions` extractor
- Role → permissions mapping
- `RoleProvider` trait (similar to `UserProvider`)
- Could be app-level initially, framework provides primitives
