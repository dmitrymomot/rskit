# Tier Module Design — 2026-03-31

Plan-based feature gating for SaaS apps. Resolves the current owner's plan and gates access to features and usage limits.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Trait pattern | `Arc<dyn TierBackend>` with `Pin<Box<dyn Future>>` | Object-safe, simple ergonomics, vtable cost negligible vs I/O |
| Owner ID extraction | Sync closure `Fn(&Parts) -> Option<String>` | Decoupled from tenant module; works with any ID source |
| Caching | None in module; app caches inside `TierBackend` impl | Keeps module simple; caching strategies vary per app |
| `require_limit` | Both route-level guard (async closure) and handler-level `check_limit()` | Guard for route enforcement; method for inline checks |
| Missing owner ID | Skip (no `TierInfo` in extensions); optional default | Guards handle absence; `.with_default()` for anonymous tiers |
| Template integration | Safe functions (`tier_has`, `tier_enabled`, `tier_limit`, `tier_name`) | Never errors on undefined features regardless of MiniJinja strict mode |
| Extractor | `TierInfo` from extensions + `Service<TierResolver>` for manual resolution | Covers both middleware path and admin use case |
| Error handling | All middleware/guard errors return `Error` | App's error handler decides rendering; never raw HTTP responses |

## Dependencies

- No external crates
- No DB dependency — app's `TierBackend` brings its own storage
- Template integration gated behind `#[cfg(feature = "tier")]` in template middleware

## Public API

### Types

```rust
/// Whether a feature is a boolean toggle or a usage limit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeatureAccess {
    /// Feature is enabled or disabled.
    Toggle(bool),
    /// Feature has a usage limit ceiling.
    Limit(u64),
}

/// Resolved tier information for an owner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierInfo {
    pub name: String,
    pub features: HashMap<String, FeatureAccess>,
}

impl TierInfo {
    /// Feature is available (Toggle=true or Limit>0).
    pub fn has_feature(&self, name: &str) -> bool;

    /// Feature is explicitly enabled (Toggle only, false for Limit).
    pub fn is_enabled(&self, name: &str) -> bool;

    /// Get the limit ceiling (Limit only, None for Toggle).
    pub fn limit(&self, name: &str) -> Option<u64>;

    /// Check current usage against limit ceiling.
    /// Returns Err(Error::forbidden("Limit exceeded for 'X': 150/100")) if over.
    /// Returns Err(Error::forbidden("Feature 'X' is not available...")) if feature missing.
    /// Returns Err(Error::internal("Feature 'X' is not a limit")) if feature is a Toggle.
    pub fn check_limit(&self, name: &str, current: u64) -> Result<()>;
}
```

### Backend Trait & Resolver

```rust
/// Backend trait for tier resolution. Object-safe.
/// App implements this with its own storage/logic.
pub trait TierBackend: Send + Sync {
    fn resolve(
        &self,
        owner_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<TierInfo>> + Send + '_>>;
}

/// Concrete wrapper. Arc internally, cheap to clone.
#[derive(Clone)]
pub struct TierResolver(Arc<dyn TierBackend>);

impl TierResolver {
    /// Create from a custom backend. No built-in default.
    pub fn from_backend(backend: Arc<dyn TierBackend>) -> Self;

    /// Resolve tier info for an owner.
    pub async fn resolve(&self, owner_id: &str) -> Result<TierInfo>;
}
```

No `TierResolver::new(db)` — the framework provides the trait and infrastructure, the app owns the mapping logic.

### Middleware

```rust
/// Tower layer that resolves TierInfo and inserts it into request extensions.
pub struct TierLayer { /* private */ }

impl TierLayer {
    /// Create with a resolver and a sync closure that extracts the owner ID.
    ///
    /// The closure reads from `&Parts` (request extensions populated by upstream
    /// middleware) and returns `Some(owner_id)` or `None`.
    pub fn new<F>(resolver: TierResolver, extractor: F) -> Self
    where
        F: Fn(&Parts) -> Option<String> + Send + Sync + 'static;

    /// When the extractor returns None, inject this TierInfo instead of skipping.
    /// Useful for anonymous/unauthenticated users who get a limited tier.
    pub fn with_default(self, default: TierInfo) -> Self;
}
```

**Middleware flow:**

1. Call extractor closure with `&Parts`
2. `Some(owner_id)` — call `resolver.resolve(&owner_id).await`, insert `TierInfo` into extensions
3. `None` + default set — insert default `TierInfo` into extensions
4. `None` + no default — skip, call inner service (guards downstream handle absence)
5. Resolution error — return `Error` (app's error handler renders)

### Guards

```rust
/// Route-level feature gate. Applied with `.route_layer()`.
///
/// - TierInfo missing in extensions → Error::internal (developer misconfiguration)
/// - Feature missing or disabled → Error::forbidden("Feature 'X' is not available on your current plan")
pub fn require_feature(name: &str) -> RequireFeatureLayer;

/// Route-level limit gate with async usage closure. Applied with `.route_layer()`.
///
/// The closure receives `&Parts` and returns the current usage count.
///
/// - TierInfo missing → Error::internal
/// - Feature not a Limit → Error::internal
/// - Usage >= limit → Error::forbidden("Limit exceeded for 'X': 150/100")
pub fn require_limit<F, Fut>(name: &str, usage: F) -> RequireLimitLayer
where
    F: Fn(&Parts) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<u64>> + Send;
```

All guard errors go through `Error` — the app's error handler decides rendering.

### Extractors

```rust
/// Pull TierInfo from request extensions (populated by TierLayer).
/// Returns Error::internal("Tier middleware not applied") if missing.
impl<S: Send + Sync> FromRequestParts<S> for TierInfo { .. }

/// Optional variant — returns None if TierLayer not applied or extractor returned None.
impl<S: Send + Sync> OptionalFromRequestParts<S> for TierInfo { .. }

/// Manual resolution via Service<TierResolver> for admin use cases.
/// Requires TierResolver registered with .with_service(resolver).
// Service<TierResolver> — standard modo DI extractor
```

### Template Integration

When both `templates` and `tier` features are enabled, `TemplateContextMiddleware` auto-injects tier functions into template context. Gated with `#[cfg(feature = "tier")]`.

**Injected template globals:**

| Name | Type | Description |
|------|------|-------------|
| `tier_name` | `String` | Current plan name (e.g., "free", "pro") |
| `tier_has(name)` | `fn(&str) -> bool` | Feature available? (Toggle=true or Limit>0) |
| `tier_enabled(name)` | `fn(&str) -> bool` | Feature enabled? (Toggle only) |
| `tier_limit(name)` | `fn(&str) -> Option<u64>` | Limit ceiling (Limit only) |

All functions return safe defaults for undefined features — `false` for booleans, `None` for limits. No template errors regardless of MiniJinja's undefined behavior setting.

**Implementation** (inside `TemplateContextMiddleware::call`):

```rust
#[cfg(feature = "tier")]
if let Some(tier_info) = parts.extensions.get::<crate::tier::TierInfo>() {
    ctx.set("tier_name", minijinja::Value::from(tier_info.name.clone()));

    let ti = tier_info.clone();
    ctx.set("tier_has", minijinja::Value::from_function(
        move |name: &str| -> bool { ti.has_feature(name) },
    ));

    let ti = tier_info.clone();
    ctx.set("tier_enabled", minijinja::Value::from_function(
        move |name: &str| -> bool { ti.is_enabled(name) },
    ));

    let ti = tier_info.clone();
    ctx.set("tier_limit", minijinja::Value::from_function(
        move |name: &str| -> Option<u64> { ti.limit(name) },
    ));
}
```

**Layer ordering:** `TemplateContextLayer` must run after `TierLayer`.

**Template usage:**

```html
<span class="badge">{{ tier_name }}</span>

{% if tier_has("sso") %}
  <a href="/settings/sso">SSO Settings</a>
{% endif %}

{% if tier_has("api_calls") %}
  <p>API limit: {{ tier_limit("api_calls") }}</p>
{% endif %}
```

## Usage Examples

### Multi-Tenant App

```rust
// --- App's backend ---
struct MyTierBackend { db: Database }

impl TierBackend for MyTierBackend {
    fn resolve(&self, owner_id: &str) -> Pin<Box<dyn Future<Output = Result<TierInfo>> + Send + '_>> {
        Box::pin(async move {
            let plan: String = self.db.conn()
                .query_one_map(
                    "SELECT plan FROM tenants WHERE id = ?1",
                    libsql::params![owner_id],
                    |row| row.get::<String>(0),
                ).await?;

            Ok(match plan.as_str() {
                "free" => TierInfo {
                    name: "free".into(),
                    features: HashMap::from([
                        ("basic_export".into(), FeatureAccess::Toggle(true)),
                        ("sso".into(), FeatureAccess::Toggle(false)),
                        ("api_calls".into(), FeatureAccess::Limit(1_000)),
                    ]),
                },
                "pro" => TierInfo {
                    name: "pro".into(),
                    features: HashMap::from([
                        ("basic_export".into(), FeatureAccess::Toggle(true)),
                        ("custom_domain".into(), FeatureAccess::Toggle(true)),
                        ("sso".into(), FeatureAccess::Toggle(true)),
                        ("api_calls".into(), FeatureAccess::Limit(100_000)),
                        ("storage_mb".into(), FeatureAccess::Limit(5_000)),
                    ]),
                },
                _ => return Err(Error::not_found("Unknown plan")),
            })
        })
    }
}

// --- Wiring ---
let resolver = TierResolver::from_backend(Arc::new(MyTierBackend { db: db.clone() }));

let app = Router::new()
    // Route with feature gate
    .route("/settings/domain", get(domain_settings))
    .route_layer(tier::require_feature("custom_domain"))

    // Route with limit gate (async usage closure)
    .route("/api/embeddings", post(create_embedding))
    .route_layer(tier::require_limit("api_calls", |parts| async move {
        let db = parts.extensions.get::<Database>().unwrap();
        count_api_calls(db).await
    }))

    // Tier middleware: extract owner ID from TenantId in extensions
    .layer(TierLayer::new(resolver.clone(), |parts| {
        parts.extensions.get::<TenantId>().map(|id| id.as_str().to_owned())
    }))
    .layer(TenantLayer::new(strategy, tenant_resolver))
    .with_service(resolver);
```

### Single-Tenant, User-Level Plans

```rust
let app = Router::new()
    .route("/dashboard", get(dashboard))
    .route_layer(tier::require_feature("analytics"))
    .layer(TierLayer::new(resolver.clone(), |parts| {
        parts.extensions.get::<Session>().map(|s| s.user_id().to_owned())
    }))
    .with_service(resolver);
```

### Anonymous Default Tier

```rust
let anonymous = TierInfo {
    name: "anonymous".into(),
    features: HashMap::from([
        ("public_api".into(), FeatureAccess::Toggle(true)),
        ("api_calls".into(), FeatureAccess::Limit(10)),
    ]),
};

let app = Router::new()
    .route("/api/search", get(search))
    .layer(TierLayer::new(resolver, |parts| {
        parts.extensions.get::<TenantId>().map(|id| id.as_str().to_owned())
    }).with_default(anonymous));
```

### Handler-Level Limit Check

```rust
async fn upload_file(
    tier: TierInfo,
    Service(db): Service<Database>,
) -> Result<()> {
    let current_storage = get_storage_usage(&db).await?;
    tier.check_limit("storage_mb", current_storage)?;
    // proceed with upload
    Ok(())
}
```

### Admin Resolving Another Tenant's Tier

```rust
async fn admin_view_plan(
    Service(resolver): Service<TierResolver>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TierInfo>> {
    let info = resolver.resolve(&tenant_id).await?;
    Ok(Json(info))
}
```

### Template Usage

```html
<nav>
  <span class="plan-badge">{{ tier_name }}</span>

  {% if tier_has("custom_domain") %}
    <a href="/settings/domain">Custom Domain</a>
  {% endif %}

  {% if tier_has("sso") %}
    <a href="/settings/sso">SSO</a>
  {% else %}
    <a href="/upgrade" class="upgrade">Upgrade for SSO</a>
  {% endif %}

  {% if tier_has("api_calls") %}
    <p>API call limit: {{ tier_limit("api_calls") }}</p>
  {% endif %}
</nav>
```

## File Structure

```
src/tier/
├── mod.rs          — pub mod + re-exports
├── types.rs        — FeatureAccess, TierInfo, TierBackend, TierResolver
├── middleware.rs    — TierLayer, TierMiddleware
├── guard.rs        — require_feature(), require_limit()
├── extractor.rs    — TierInfo FromRequestParts + OptionalFromRequestParts
```

## Feature Flag

- **Flag:** `tier` — no dependencies, no DB requirement
- **In `Cargo.toml`:** `tier = []`, added to `full` feature set
- **In `lib.rs`:** `#[cfg(feature = "tier")] pub mod tier;`
- **Template integration:** `#[cfg(feature = "tier")]` block in `src/template/middleware.rs`

## Error Handling

| Situation | Error |
|-----------|-------|
| `TierBackend::resolve` fails | `Error` from backend (app-defined) |
| Guard: `TierInfo` missing in extensions | `Error::internal("require_feature() called without TierLayer")` |
| Guard: feature missing or disabled | `Error::forbidden("Feature 'X' is not available on your current plan")` |
| Guard: limit exceeded | `Error::forbidden("Limit exceeded for 'X': 150/100")` |
| Guard: feature is not a Limit | `Error::internal("Feature 'X' is not a limit")` |
| Extractor: `TierInfo` missing | `Error::internal("Tier middleware not applied")` |

All errors return `Error` — the app's error handler decides rendering. Guards and middleware never construct raw HTTP responses.

## Testing Strategy

- **Unit tests:** `TierInfo` methods (`has_feature`, `is_enabled`, `limit`, `check_limit`)
- **Unit tests:** `FeatureAccess` serialization/deserialization
- **Middleware tests:** Using in-memory `TierBackend` impl with `tower::ServiceExt::oneshot`
  - Extractor returns `Some` → `TierInfo` in extensions
  - Extractor returns `None` → no `TierInfo`, inner service called
  - Extractor returns `None` + default → default `TierInfo` in extensions
  - Backend error → error response
- **Guard tests:**
  - `require_feature` with present/missing/disabled features
  - `require_limit` with under/over/equal usage
  - Both guards with missing `TierInfo` → internal error
- **Extractor tests:** `TierInfo` from extensions, missing extensions
- **Test helper:** `#[cfg(any(test, feature = "test-helpers"))]` in-memory backend

No integration tests needed — no DB, no external dependencies. All tests are unit tests with in-memory backends.
