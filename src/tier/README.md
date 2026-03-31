# tier

Tier-based feature gating for SaaS applications.

Requires feature `"tier"`.

```toml
[dependencies]
modo = { version = "0.3", features = ["tier"] }
```

## Key types

| Type | Purpose |
|------|---------|
| [`TierBackend`] | Trait for pluggable tier resolution (app implements) |
| [`TierResolver`] | Concrete wrapper (`Arc<dyn TierBackend>`, cheap to clone) |
| [`TierInfo`] | Resolved tier with feature-check helpers |
| [`FeatureAccess`] | Toggle or usage-limit feature model |
| [`TierLayer`] | Tower middleware that resolves and injects `TierInfo` |
| [`require_feature()`] | Route guard for boolean feature gates |
| [`require_limit()`] | Route guard for usage-limit gates |

## Usage

### Implement `TierBackend`

The app provides its own storage/logic behind the `TierBackend` trait.
The trait is object-safe (`Pin<Box<dyn Future>>`, not RPITIT).

```rust,ignore
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use modo::tier::{TierBackend, TierResolver, TierInfo, FeatureAccess};
use modo::Result;

struct MyTierBackend { /* db handle, cache, etc. */ }

impl TierBackend for MyTierBackend {
    fn resolve(
        &self,
        owner_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<TierInfo>> + Send + '_>> {
        Box::pin(async move {
            Ok(TierInfo {
                name: "pro".into(),
                features: HashMap::from([
                    ("sso".into(), FeatureAccess::Toggle(true)),
                    ("api_calls".into(), FeatureAccess::Limit(100_000)),
                ]),
            })
        })
    }
}

let resolver = TierResolver::from_backend(Arc::new(MyTierBackend { /* ... */ }));
```

### Wire the middleware and guards

`TierLayer` resolves the tier and inserts `TierInfo` into request extensions.
Guards run after route matching via `.route_layer()`.

```rust,ignore
use modo::tier::{TierLayer, TierResolver, TierInfo, require_feature, require_limit};
use axum::{Router, routing::get};

fn app(resolver: TierResolver) -> Router {
    Router::new()
        // Boolean gate: reject unless the feature is available.
        .route("/settings/domain", get(custom_domain_handler))
        .route_layer(require_feature("custom_domain"))

        // Usage-limit gate: reject when current usage >= ceiling.
        .route("/api/widgets", get(widgets_handler))
        .route_layer(require_limit("api_calls", |parts| async {
            // Return current usage count (e.g., from a counter in extensions).
            Ok(0u64)
        }))

        // Tier middleware — must be outermost so TierInfo is available to guards.
        .layer(TierLayer::new(resolver, |parts| {
            parts.extensions
                .get::<modo::TenantId>()
                .map(|id| id.as_str().to_owned())
        }))
}
```

### Extract `TierInfo` in handlers

`TierInfo` implements `FromRequestParts` and `OptionalFromRequestParts`,
so it can be used directly as an axum extractor.

```rust,ignore
use modo::tier::TierInfo;

async fn handler(tier: TierInfo) -> String {
    if tier.has_feature("sso") {
        "SSO enabled".into()
    } else {
        "SSO not available".into()
    }
}

// Optional extraction — returns None when TierLayer is not applied.
async fn optional_handler(tier: Option<TierInfo>) -> String {
    match tier {
        Some(t) => format!("Plan: {}", t.name),
        None => "No tier info".into(),
    }
}
```

### Check features and limits programmatically

`TierInfo` provides helpers for both toggle and limit features:

```rust,ignore
use modo::tier::{TierInfo, FeatureAccess};

fn check_tier(tier: &TierInfo) -> modo::Result<()> {
    // Boolean check — true for Toggle(true) or Limit(>0).
    assert!(tier.has_feature("sso"));

    // Strict toggle check — false for Limit features.
    assert!(tier.is_enabled("sso"));

    // Raw limit ceiling — None for Toggle or missing features.
    let ceiling: Option<u64> = tier.limit("api_calls");

    // Limit ceiling with typed errors.
    let ceiling: u64 = tier.limit_ceiling("api_calls")?;

    // Usage check — errors if current >= ceiling.
    tier.check_limit("api_calls", 42)?;

    Ok(())
}
```

### Default tier for unauthenticated requests

When the owner extractor returns `None`, you can inject a fallback tier
instead of leaving `TierInfo` absent:

```rust,ignore
use std::collections::HashMap;
use modo::tier::{TierLayer, TierInfo, FeatureAccess};

let anon_tier = TierInfo {
    name: "anonymous".into(),
    features: HashMap::from([
        ("public_api".into(), FeatureAccess::Toggle(true)),
    ]),
};

let layer = TierLayer::new(resolver, |parts| {
    parts.extensions.get::<modo::TenantId>().map(|id| id.as_str().to_owned())
}).with_default(anon_tier);
```

## Error handling

| Error | HTTP status | When |
|-------|-------------|------|
| Tier middleware not applied | 500 Internal Server Error | `TierInfo` extracted but `TierLayer` is missing |
| Backend resolution failure | 500 Internal Server Error | `TierBackend::resolve` returns an error |
| Feature missing or disabled | 403 Forbidden | `require_feature` gate rejects the request |
| Feature is not a limit | 500 Internal Server Error | `require_limit` used on a `Toggle` feature |
| Usage >= limit | 403 Forbidden | `require_limit` gate rejects the request |
| Usage closure error | Depends on error | `require_limit` usage closure returns an error |

## Test helpers

The `modo::tier::test` submodule provides in-memory backends for testing:

| Type | Purpose |
|------|---------|
| `StaticTierBackend` | Returns a fixed `TierInfo` for any owner ID |
| `FailingTierBackend` | Always returns an internal error |

Available when running tests or when the `test-helpers` feature is enabled.
