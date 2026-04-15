# Tier

Tier-based feature gating for SaaS applications. Always available.

Import directly from `modo::tier`:

```rust
use modo::tier::{
    FeatureAccess, TierBackend, TierInfo, TierLayer, TierResolver,
    require_feature, require_limit,
};
```

Wiring-site shortcuts are also available: `modo::middlewares::Tier`,
`modo::guards::require_feature`, `modo::guards::require_limit`.

Test backends are available under `#[cfg(test)]` or `feature = "test-helpers"`:

```rust
use modo::tier::test::{StaticTierBackend, FailingTierBackend};
```

Source: `src/tier/` (mod.rs, types.rs, extractor.rs, middleware.rs, guard.rs).

---

## FeatureAccess

Whether a feature is a boolean toggle or a usage limit.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureAccess {
    Toggle(bool),
    Limit(u64),
}
```

- `Toggle(true)` — feature is enabled.
- `Toggle(false)` — feature is disabled.
- `Limit(n)` — feature has a usage ceiling of `n`. A limit of `0` means effectively disabled.

---

## TierInfo

Resolved tier information for an owner. Inserted into request extensions by `TierLayer` and extracted by handlers via `FromRequestParts`.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TierInfo {
    pub name: String,
    pub features: HashMap<String, FeatureAccess>,
}
```

- `name` — plan name (e.g., `"free"`, `"pro"`, `"enterprise"`).
- `features` — feature map: feature name → access level.

### has_feature(&self, name: &str) -> bool

Returns `true` if the feature exists and is available: `Toggle(true)` or `Limit(n)` where `n > 0`. Returns `false` for `Toggle(false)`, `Limit(0)`, or missing features.

### is_enabled(&self, name: &str) -> bool

Returns `true` only for `Toggle(true)`. Returns `false` for `Toggle(false)`, any `Limit`, or missing features. Use this when you specifically need to check a boolean toggle, not a limit.

### limit(&self, name: &str) -> Option\<u64\>

Returns `Some(ceiling)` for `Limit` features, `None` for `Toggle` or missing features.

### limit_ceiling(&self, name: &str) -> Result\<u64\>

Like `limit()` but returns typed errors instead of `None`:

- `Error::forbidden` if the feature is missing.
- `Error::internal` if the feature is a `Toggle` (not a limit).

Used internally by `require_limit` guard and `check_limit`.

### check_limit(&self, name: &str, current: u64) -> Result\<()\>

Checks current usage against the limit ceiling. Delegates to `limit_ceiling()` for ceiling extraction.

- Returns `Ok(())` if `current < ceiling`.
- Returns `Error::forbidden` if `current >= ceiling` or feature is missing.
- Returns `Error::internal` if the feature is a `Toggle`.

### FromRequestParts

`TierInfo` implements `FromRequestParts` — extract it directly in handler parameters. Returns `Error::internal("Tier middleware not applied")` (500) if `TierInfo` is not in extensions.

Also implements `OptionalFromRequestParts` — returns `Ok(None)` if `TierInfo` is absent, never errors.

---

## TierBackend

Object-safe trait for tier resolution. The app implements this with its own storage/logic.

```rust
pub trait TierBackend: Send + Sync {
    fn resolve(
        &self,
        owner_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<TierInfo>> + Send + '_>>;
}
```

Uses `Pin<Box<dyn Future>>` for object safety (not RPITIT). Wrap in `Arc<dyn TierBackend>` and pass to `TierResolver::from_backend()`.

---

## TierResolver

Concrete wrapper around `Arc<dyn TierBackend>`. Cheap to clone.

```rust
#[derive(Clone)]
pub struct TierResolver(Arc<dyn TierBackend>);
```

### from_backend(backend: Arc\<dyn TierBackend\>) -> Self

Create a resolver from a custom backend.

### async resolve(&self, owner_id: &str) -> Result\<TierInfo\>

Delegates to the underlying `TierBackend::resolve()`.

---

## TierLayer

Tower middleware layer that resolves `TierInfo` and inserts it into request extensions. Apply with `.layer()` on the router.

```rust
pub struct TierLayer { /* private fields */ }
```

### new\<F\>(resolver: TierResolver, extractor: F) -> Self

Create a new tier layer. The `extractor` is a sync closure that returns the owner ID from request parts, or `None` if no owner context is available.

```rust
where F: Fn(&Parts) -> Option<String> + Send + Sync + 'static
```

### with_default(self, default: TierInfo) -> Self

When the extractor returns `None`, inject this `TierInfo` instead of skipping. Useful for anonymous/unauthenticated users who should get a "free" tier.

### Behavior

1. Calls `extractor(&parts)` to get the owner ID.
2. If `Some(owner_id)`: calls `resolver.resolve(&owner_id)`. On success, inserts `TierInfo` into extensions. On error, returns the error as an HTTP response (via `Error::into_response()`).
3. If `None` and `with_default` was set: inserts the default `TierInfo`.
4. If `None` and no default: does nothing — downstream guards handle the absence.

---

## require_feature(name: &str) -> RequireFeatureLayer

Route guard that rejects requests unless the resolved tier includes the named feature and it is available (per `TierInfo::has_feature()`).

Apply with `.route_layer()` so it runs after route matching. `TierLayer` must be applied with `.layer()` upstream.

- `TierInfo` missing → `Error::internal` (500) — developer misconfiguration.
- Feature missing or disabled → `Error::forbidden` (403).

```rust
Router::new()
    .route("/settings/sso", get(sso_handler))
    .route_layer(require_feature("sso"))
```

---

## require_limit\<F, Fut\>(name: &str, usage: F) -> RequireLimitLayer\<F\>

Route guard that rejects requests when current usage meets or exceeds the tier's limit ceiling.

```rust
where
    F: Fn(&Parts) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<u64>> + Send,
```

The `usage` closure receives `&Parts` and returns the current usage count asynchronously.

Apply with `.route_layer()`. `TierLayer` must be applied upstream.

- `TierInfo` missing → `Error::internal` (500).
- Feature is a `Toggle` (not a `Limit`) → `Error::internal` (500).
- Feature missing → `Error::forbidden` (403).
- Ceiling is `0` → `Error::forbidden` (403) — short-circuits, does not call `usage`.
- `usage >= ceiling` → `Error::forbidden` (403).
- `usage` closure returns error → surfaces that error.

```rust
Router::new()
    .route("/api/data", post(create_data))
    .route_layer(require_limit("api_calls", |parts| {
        let db = parts.extensions.get::<Database>().unwrap().clone();
        let owner = parts.extensions.get::<OwnerId>().unwrap().0.clone();
        async move { count_api_calls(&db, &owner).await }
    }))
```

---

## Test Backends

Available under `#[cfg(test)]` or `feature = "test-helpers"`.

### StaticTierBackend

Returns a fixed `TierInfo` for any owner ID.

```rust
pub struct StaticTierBackend { /* private */ }
```

#### new(tier: TierInfo) -> Self

Create a backend that always returns the given tier.

### FailingTierBackend

Always returns `Error::internal("test: backend failure")`.

```rust
pub struct FailingTierBackend;
```

---

## Gotchas

- **Layer order matters.** `TierLayer` must be applied with `.layer()` (runs for all requests). Guards (`require_feature`, `require_limit`) must be applied with `.route_layer()` (runs only for matched routes). If reversed, guards see no `TierInfo` and return 500.
- **Guards are fail-closed.** Missing `TierInfo` → 500, not 403. This is deliberate — it signals a wiring bug, not a permissions issue.
- **`has_feature` treats `Limit(0)` as disabled.** A feature with `Limit(0)` returns `false` from `has_feature()` and will be rejected by `require_feature()`. This is intentional — zero-limit means the feature is not available on the plan.
- **`require_limit` short-circuits at ceiling 0.** When the limit ceiling is `0`, the guard returns 403 immediately without calling the `usage` closure. This avoids a wasted database query.
- **Template functions** (`tier_name`, `tier_has`, `tier_enabled`, `tier_limit`) are injected by `TemplateContextLayer` (feature `templates`) when `TierInfo` is in request extensions. No extra setup needed beyond applying `TierLayer`.
