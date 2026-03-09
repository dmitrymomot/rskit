# modo-tenant Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add multi-tenancy with RBAC to modo — tenant resolution, membership, role guards, and template context injection.

**Architecture:** Three deliverables: (1) `modo-tenant` crate with traits, extractors, built-in resolvers, context layer, and guard functions; (2) `modo-tenant-macros` crate with `#[allow_roles]`/`#[deny_roles]` proc macros; (3) `UserContextLayer` in `modo-auth` for template user injection. All follow existing modo patterns (service registry, type-erased providers, extension caching).

**Tech Stack:** Rust, axum 0.8, Tower layers, syn/quote proc macros, minijinja (template context), serde (serialization bounds)

---

## Task 1: Scaffold `modo-tenant` and `modo-tenant-macros` crates

**Files:**
- Create: `modo-tenant/Cargo.toml`
- Create: `modo-tenant/src/lib.rs`
- Create: `modo-tenant-macros/Cargo.toml`
- Create: `modo-tenant-macros/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Step 1: Create `modo-tenant/Cargo.toml`**

```toml
[package]
name = "modo-tenant"
version = "0.1.0"
edition = "2024"
license.workspace = true

[dependencies]
modo = { path = "../modo" }
modo-session = { path = "../modo-session" }
serde = { version = "1", features = ["derive"] }
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["full"] }
axum = "0.8"
axum-extra = { version = "0.10", features = ["cookie-signed"] }
tower = { version = "0.5", features = ["util"] }
http = "1"
serde_json = "1"
```

**Step 2: Create `modo-tenant/src/lib.rs`**

```rust
pub mod resolver;

pub use resolver::HasTenantId;
```

**Step 3: Create `modo-tenant-macros/Cargo.toml`**

```toml
[package]
name = "modo-tenant-macros"
version = "0.1.0"
edition = "2024"
license.workspace = true

[lib]
proc-macro = true

[dependencies]
syn = { version = "2", features = ["full", "extra-traits"] }
quote = "1"
proc-macro2 = "1"
```

**Step 4: Create `modo-tenant-macros/src/lib.rs`**

```rust
use proc_macro::TokenStream;

/// Placeholder — implemented in Task 8.
#[proc_macro_attribute]
pub fn allow_roles(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Placeholder — implemented in Task 8.
#[proc_macro_attribute]
pub fn deny_roles(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
```

**Step 5: Add both crates to workspace `Cargo.toml`**

Add `"modo-tenant"` and `"modo-tenant-macros"` to the `members` list.

**Step 6: Verify it compiles**

Run: `cargo check -p modo-tenant -p modo-tenant-macros`
Expected: compiles with no errors

**Step 7: Commit**

```bash
git add modo-tenant/ modo-tenant-macros/ Cargo.toml Cargo.lock
git commit -m "$(cat <<'EOF'
chore: scaffold modo-tenant and modo-tenant-macros crates
EOF
)"
```

---

## Task 2: `HasTenantId` trait and `TenantResolver` trait + service wrapper

**Files:**
- Create: `modo-tenant/src/resolver.rs`
- Modify: `modo-tenant/src/lib.rs`

**Step 1: Write tests for `TenantResolverService`**

Add to bottom of `modo-tenant/src/resolver.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use modo::axum::http::request::Parts;
    use modo::axum::http::Request;

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct TestTenant {
        id: String,
        name: String,
    }

    impl HasTenantId for TestTenant {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    struct TestResolver;

    impl TenantResolver for TestResolver {
        type Tenant = TestTenant;

        async fn resolve(
            &self,
            parts: &Parts,
        ) -> Result<Option<Self::Tenant>, modo::Error> {
            let host = parts
                .headers
                .get("host")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if host == "acme.test.com" {
                Ok(Some(TestTenant {
                    id: "t-1".to_string(),
                    name: "Acme".to_string(),
                }))
            } else if host == "error.test.com" {
                Err(modo::Error::internal("db error"))
            } else {
                Ok(None)
            }
        }
    }

    fn test_parts(host: &str) -> Parts {
        let req = Request::builder()
            .header("host", host)
            .body(())
            .unwrap();
        req.into_parts().0
    }

    #[tokio::test]
    async fn resolver_service_finds_tenant() {
        let svc = TenantResolverService::new(TestResolver);
        let parts = test_parts("acme.test.com");
        let tenant = svc.resolve(&parts).await.unwrap();
        assert_eq!(
            tenant,
            Some(TestTenant {
                id: "t-1".to_string(),
                name: "Acme".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn resolver_service_returns_none_for_unknown() {
        let svc = TenantResolverService::new(TestResolver);
        let parts = test_parts("unknown.test.com");
        let tenant = svc.resolve(&parts).await.unwrap();
        assert_eq!(tenant, None);
    }

    #[tokio::test]
    async fn resolver_service_propagates_errors() {
        let svc = TenantResolverService::new(TestResolver);
        let parts = test_parts("error.test.com");
        let result = svc.resolve(&parts).await;
        assert!(result.is_err());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-tenant`
Expected: FAIL — `TenantResolver`, `HasTenantId`, `TenantResolverService` not defined

**Step 3: Implement `HasTenantId`, `TenantResolver`, and `TenantResolverService`**

Write `modo-tenant/src/resolver.rs`:

```rust
use modo::axum::http::request::Parts;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Trait that tenant types must implement to expose their ID.
pub trait HasTenantId {
    fn tenant_id(&self) -> &str;
}

/// Pluggable tenant resolution from HTTP request parts.
pub trait TenantResolver: Send + Sync + 'static {
    type Tenant: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static;

    fn resolve(
        &self,
        parts: &Parts,
    ) -> impl Future<Output = Result<Option<Self::Tenant>, modo::Error>> + Send;
}

// Object-safe bridge trait for type erasure
trait TenantResolverDyn<T>: Send + Sync {
    fn resolve<'a>(
        &'a self,
        parts: &'a Parts,
    ) -> Pin<Box<dyn Future<Output = Result<Option<T>, modo::Error>> + Send + 'a>>;
}

impl<R: TenantResolver> TenantResolverDyn<R::Tenant> for R {
    fn resolve<'a>(
        &'a self,
        parts: &'a Parts,
    ) -> Pin<Box<dyn Future<Output = Result<Option<R::Tenant>, modo::Error>> + Send + 'a>> {
        Box::pin(TenantResolver::resolve(self, parts))
    }
}

/// Type-erased wrapper stored in the service registry.
pub struct TenantResolverService<T: Clone + Send + Sync + 'static> {
    inner: Arc<dyn TenantResolverDyn<T>>,
}

impl<T: Clone + Send + Sync + 'static> Clone for TenantResolverService<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Clone + Send + Sync + 'static> TenantResolverService<T> {
    pub fn new<R: TenantResolver<Tenant = T>>(resolver: R) -> Self {
        Self {
            inner: Arc::new(resolver),
        }
    }

    pub async fn resolve(&self, parts: &Parts) -> Result<Option<T>, modo::Error> {
        self.inner.resolve(parts).await
    }
}
```

**Step 4: Update `modo-tenant/src/lib.rs`**

```rust
pub mod resolver;

pub use resolver::{HasTenantId, TenantResolver, TenantResolverService};
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p modo-tenant`
Expected: 3 tests pass

**Step 6: Commit**

```bash
git add modo-tenant/src/
git commit -m "$(cat <<'EOF'
feat(modo-tenant): add HasTenantId trait, TenantResolver trait, and TenantResolverService
EOF
)"
```

---

## Task 3: `MemberProvider` trait + service wrapper

**Files:**
- Create: `modo-tenant/src/member.rs`
- Modify: `modo-tenant/src/lib.rs`

**Step 1: Write tests for `MemberProviderService`**

Add to `modo-tenant/src/member.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct TestMember {
        user_id: String,
        tenant_id: String,
        role: String,
    }

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct TestTenant {
        id: String,
        name: String,
    }

    impl crate::HasTenantId for TestTenant {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    struct TestMemberProvider;

    impl MemberProvider for TestMemberProvider {
        type Member = TestMember;
        type Tenant = TestTenant;

        async fn find_member(
            &self,
            user_id: &str,
            tenant_id: &str,
        ) -> Result<Option<Self::Member>, modo::Error> {
            if user_id == "u-1" && tenant_id == "t-1" {
                Ok(Some(TestMember {
                    user_id: "u-1".to_string(),
                    tenant_id: "t-1".to_string(),
                    role: "admin".to_string(),
                }))
            } else if user_id == "error" {
                Err(modo::Error::internal("db error"))
            } else {
                Ok(None)
            }
        }

        async fn list_tenants(
            &self,
            user_id: &str,
        ) -> Result<Vec<Self::Tenant>, modo::Error> {
            if user_id == "u-1" {
                Ok(vec![
                    TestTenant { id: "t-1".to_string(), name: "Acme".to_string() },
                    TestTenant { id: "t-2".to_string(), name: "Beta".to_string() },
                ])
            } else {
                Ok(vec![])
            }
        }

        fn role(&self, member: &Self::Member) -> &str {
            &member.role
        }
    }

    #[tokio::test]
    async fn member_provider_finds_member() {
        let svc = MemberProviderService::new(TestMemberProvider);
        let member = svc.find_member("u-1", "t-1").await.unwrap();
        assert!(member.is_some());
        let m = member.unwrap();
        assert_eq!(svc.role(&m), "admin");
    }

    #[tokio::test]
    async fn member_provider_returns_none_for_non_member() {
        let svc = MemberProviderService::new(TestMemberProvider);
        let member = svc.find_member("u-1", "t-999").await.unwrap();
        assert!(member.is_none());
    }

    #[tokio::test]
    async fn member_provider_lists_tenants() {
        let svc = MemberProviderService::new(TestMemberProvider);
        let tenants = svc.list_tenants("u-1").await.unwrap();
        assert_eq!(tenants.len(), 2);
    }

    #[tokio::test]
    async fn member_provider_propagates_errors() {
        let svc = MemberProviderService::new(TestMemberProvider);
        let result = svc.find_member("error", "t-1").await;
        assert!(result.is_err());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p modo-tenant`
Expected: FAIL — `MemberProvider`, `MemberProviderService` not defined

**Step 3: Implement `MemberProvider` and `MemberProviderService`**

Write `modo-tenant/src/member.rs`:

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Loads membership records and tenant lists for a user.
pub trait MemberProvider: Send + Sync + 'static {
    type Member: Clone + Send + Sync + serde::Serialize + 'static;
    type Tenant: Clone + Send + Sync + crate::HasTenantId + serde::Serialize + 'static;

    fn find_member(
        &self,
        user_id: &str,
        tenant_id: &str,
    ) -> impl Future<Output = Result<Option<Self::Member>, modo::Error>> + Send;

    fn list_tenants(
        &self,
        user_id: &str,
    ) -> impl Future<Output = Result<Vec<Self::Tenant>, modo::Error>> + Send;

    fn role(&self, member: &Self::Member) -> &str;
}

// Object-safe bridge
trait MemberProviderDyn<M, T>: Send + Sync {
    fn find_member<'a>(
        &'a self,
        user_id: &'a str,
        tenant_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<M>, modo::Error>> + Send + 'a>>;

    fn list_tenants<'a>(
        &'a self,
        user_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<T>, modo::Error>> + Send + 'a>>;

    fn role<'a>(&'a self, member: &'a M) -> &'a str;
}

impl<P: MemberProvider> MemberProviderDyn<P::Member, P::Tenant> for P {
    fn find_member<'a>(
        &'a self,
        user_id: &'a str,
        tenant_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<P::Member>, modo::Error>> + Send + 'a>> {
        Box::pin(MemberProvider::find_member(self, user_id, tenant_id))
    }

    fn list_tenants<'a>(
        &'a self,
        user_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<P::Tenant>, modo::Error>> + Send + 'a>> {
        Box::pin(MemberProvider::list_tenants(self, user_id))
    }

    fn role<'a>(&'a self, member: &'a P::Member) -> &'a str {
        MemberProvider::role(self, member)
    }
}

/// Type-erased wrapper stored in the service registry.
pub struct MemberProviderService<M: Clone + Send + Sync + 'static, T: Clone + Send + Sync + 'static> {
    inner: Arc<dyn MemberProviderDyn<M, T>>,
}

impl<M: Clone + Send + Sync + 'static, T: Clone + Send + Sync + 'static> Clone
    for MemberProviderService<M, T>
{
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<M: Clone + Send + Sync + 'static, T: Clone + Send + Sync + 'static>
    MemberProviderService<M, T>
{
    pub fn new<P: MemberProvider<Member = M, Tenant = T>>(provider: P) -> Self {
        Self {
            inner: Arc::new(provider),
        }
    }

    pub async fn find_member(
        &self,
        user_id: &str,
        tenant_id: &str,
    ) -> Result<Option<M>, modo::Error> {
        self.inner.find_member(user_id, tenant_id).await
    }

    pub async fn list_tenants(&self, user_id: &str) -> Result<Vec<T>, modo::Error> {
        self.inner.list_tenants(user_id).await
    }

    pub fn role(&self, member: &M) -> &str {
        self.inner.role(member)
    }
}
```

**Step 4: Update `modo-tenant/src/lib.rs`**

```rust
pub mod member;
pub mod resolver;

pub use member::{MemberProvider, MemberProviderService};
pub use resolver::{HasTenantId, TenantResolver, TenantResolverService};
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p modo-tenant`
Expected: 7 tests pass (3 from Task 2 + 4 new)

**Step 6: Commit**

```bash
git add modo-tenant/src/
git commit -m "$(cat <<'EOF'
feat(modo-tenant): add MemberProvider trait and MemberProviderService
EOF
)"
```

---

## Task 4: `Tenant<T>` and `OptionalTenant<T>` extractors

**Files:**
- Create: `modo-tenant/src/extractor.rs`
- Create: `modo-tenant/src/cache.rs`
- Modify: `modo-tenant/src/lib.rs`

**Step 1: Create the extension cache types**

Write `modo-tenant/src/cache.rs`:

```rust
use std::sync::Arc;

/// Cached resolved tenant in request extensions.
pub struct ResolvedTenant<T>(pub Arc<T>);

impl<T> Clone for ResolvedTenant<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Cached resolved member in request extensions.
pub struct ResolvedMember<M>(pub Arc<M>);

impl<M> Clone for ResolvedMember<M> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Cached resolved role in request extensions.
#[derive(Clone)]
pub struct ResolvedRole(pub String);

/// Cached resolved tenants list in request extensions.
pub struct ResolvedTenants<T>(pub Arc<Vec<T>>);

impl<T> Clone for ResolvedTenants<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
```

**Step 2: Write tests for `Tenant<T>` and `OptionalTenant<T>` extractors**

These require the Tower oneshot pattern used in modo tests. Write in `modo-tenant/src/extractor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::TenantResolverService;
    use crate::cache::ResolvedTenant;
    use modo::app::{AppState, ServiceRegistry};
    use modo::axum::Router;
    use modo::axum::routing::get;
    use modo::axum::http::{Request, StatusCode};
    use modo::axum::body::Body;
    use tower::ServiceExt;

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct TestTenant {
        id: String,
        name: String,
    }

    impl crate::HasTenantId for TestTenant {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    struct TestResolver;

    impl crate::TenantResolver for TestResolver {
        type Tenant = TestTenant;

        async fn resolve(
            &self,
            parts: &modo::axum::http::request::Parts,
        ) -> Result<Option<Self::Tenant>, modo::Error> {
            let host = parts
                .headers
                .get("host")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if host.starts_with("acme.") {
                Ok(Some(TestTenant {
                    id: "t-1".to_string(),
                    name: "Acme".to_string(),
                }))
            } else {
                Ok(None)
            }
        }
    }

    fn app_state_with_resolver() -> AppState {
        let services = ServiceRegistry::new()
            .with(TenantResolverService::new(TestResolver));
        AppState {
            services,
            server_config: Default::default(),
            cookie_key: axum_extra::extract::cookie::Key::generate(),
        }
    }

    #[tokio::test]
    async fn tenant_extractor_returns_tenant() {
        let state = app_state_with_resolver();
        let app = Router::new()
            .route("/", get(|t: Tenant<TestTenant>| async move { t.name.clone() }))
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .header("host", "acme.test.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn tenant_extractor_returns_404_when_missing() {
        let state = app_state_with_resolver();
        let app = Router::new()
            .route("/", get(|_t: Tenant<TestTenant>| async { "ok" }))
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .header("host", "unknown.test.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn optional_tenant_returns_none_when_missing() {
        let state = app_state_with_resolver();
        let app = Router::new()
            .route(
                "/",
                get(|t: OptionalTenant<TestTenant>| async move {
                    if t.0.is_some() { "found" } else { "none" }
                }),
            )
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .header("host", "unknown.test.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }
}
```

**Step 3: Run tests to verify they fail**

Run: `cargo test -p modo-tenant`
Expected: FAIL — `Tenant`, `OptionalTenant` not defined

**Step 4: Implement `Tenant<T>` and `OptionalTenant<T>` extractors**

Write the implementation part of `modo-tenant/src/extractor.rs`:

```rust
use crate::cache::ResolvedTenant;
use crate::resolver::TenantResolverService;
use crate::HasTenantId;
use modo::app::AppState;
use modo::axum::extract::FromRequestParts;
use modo::axum::http::request::Parts;
use modo::{Error, HttpError};
use std::ops::Deref;
use std::sync::Arc;

/// Extractor that requires a resolved tenant. Returns 404 if not found.
#[derive(Clone)]
pub struct Tenant<T: Clone + Send + Sync + 'static>(pub T);

impl<T: Clone + Send + Sync + 'static> Deref for Tenant<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Resolve tenant from cache or via resolver service, caching the result.
pub(crate) async fn resolve_tenant<T>(
    parts: &mut Parts,
    state: &AppState,
) -> Result<Option<T>, Error>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    // Check cache first
    if let Some(cached) = parts.extensions.get::<ResolvedTenant<T>>() {
        return Ok(Some((*cached.0).clone()));
    }

    let resolver = state
        .services
        .get::<TenantResolverService<T>>()
        .ok_or_else(|| {
            Error::internal(format!(
                "TenantResolverService<{}> not registered",
                std::any::type_name::<T>()
            ))
        })?;

    let tenant = resolver.resolve(parts).await?;

    if let Some(ref t) = tenant {
        parts
            .extensions
            .insert(ResolvedTenant(Arc::new(t.clone())));
    }

    Ok(tenant)
}

impl<T> FromRequestParts<AppState> for Tenant<T>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let tenant = resolve_tenant::<T>(parts, state)
            .await?
            .ok_or_else(|| Error::from(HttpError::NotFound))?;
        Ok(Tenant(tenant))
    }
}

/// Extractor that optionally resolves a tenant. Never rejects due to missing tenant.
#[derive(Clone)]
pub struct OptionalTenant<T: Clone + Send + Sync + 'static>(pub Option<T>);

impl<T: Clone + Send + Sync + 'static> Deref for OptionalTenant<T> {
    type Target = Option<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> FromRequestParts<AppState> for OptionalTenant<T>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
{
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let tenant = resolve_tenant::<T>(parts, state).await?;
        Ok(OptionalTenant(tenant))
    }
}
```

**Step 5: Update `modo-tenant/src/lib.rs`**

```rust
pub mod cache;
pub mod extractor;
pub mod member;
pub mod resolver;

pub use extractor::{OptionalTenant, Tenant};
pub use member::{MemberProvider, MemberProviderService};
pub use resolver::{HasTenantId, TenantResolver, TenantResolverService};
```

**Step 6: Run tests to verify they pass**

Run: `cargo test -p modo-tenant`
Expected: 10 tests pass

**Step 7: Commit**

```bash
git add modo-tenant/src/
git commit -m "$(cat <<'EOF'
feat(modo-tenant): add Tenant<T> and OptionalTenant<T> extractors with extension caching
EOF
)"
```

---

## Task 5: `Member<T, M>` extractor

**Files:**
- Modify: `modo-tenant/src/extractor.rs`
- Modify: `modo-tenant/src/lib.rs`

**Step 1: Write tests for `Member<T, M>`**

Append to the `tests` module in `modo-tenant/src/extractor.rs`. These tests need session middleware, which is complex to set up. Use the simpler approach of testing the extractor logic indirectly through a Router+oneshot pattern. Since session setup requires a DB, write a focused unit test that checks the cache path and a handler-level integration test for the full flow.

For now, add a test that verifies the `Member` struct API:

```rust
    #[test]
    fn member_accessors() {
        let member = Member::<TestTenant, TestMember> {
            tenant: TestTenant {
                id: "t-1".to_string(),
                name: "Acme".to_string(),
            },
            inner: TestMember {
                user_id: "u-1".to_string(),
                tenant_id: "t-1".to_string(),
                role: "admin".to_string(),
            },
            role: "admin".to_string(),
        };

        assert_eq!(member.tenant().tenant_id(), "t-1");
        assert_eq!(member.role(), "admin");
        assert_eq!(member.user_id, "u-1"); // Deref to M
    }

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct TestMember {
        user_id: String,
        tenant_id: String,
        role: String,
    }
```

**Step 2: Implement `Member<T, M>`**

Add to `modo-tenant/src/extractor.rs`:

```rust
use crate::cache::{ResolvedMember, ResolvedRole};
use crate::member::MemberProviderService;
use modo_session::SessionManager;

/// Extractor that requires tenant + auth + membership.
/// Returns 404 (no tenant), 401 (no auth), or 403 (not a member).
pub struct Member<T: HasTenantId + Clone + Send + Sync + 'static, M: Clone + Send + Sync + 'static>
{
    tenant: T,
    inner: M,
    role: String,
}

impl<T: HasTenantId + Clone + Send + Sync + 'static, M: Clone + Send + Sync + 'static> Clone
    for Member<T, M>
{
    fn clone(&self) -> Self {
        Self {
            tenant: self.tenant.clone(),
            inner: self.inner.clone(),
            role: self.role.clone(),
        }
    }
}

impl<T: HasTenantId + Clone + Send + Sync + 'static, M: Clone + Send + Sync + 'static>
    Member<T, M>
{
    pub fn tenant(&self) -> &T {
        &self.tenant
    }

    pub fn role(&self) -> &str {
        &self.role
    }

    pub fn into_inner(self) -> M {
        self.inner
    }
}

impl<T: HasTenantId + Clone + Send + Sync + 'static, M: Clone + Send + Sync + 'static> Deref
    for Member<T, M>
{
    type Target = M;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, M> FromRequestParts<AppState> for Member<T, M>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // 1. Resolve tenant (cached or fresh)
        let tenant = resolve_tenant::<T>(parts, state)
            .await?
            .ok_or_else(|| Error::from(HttpError::NotFound))?;

        // 2. Check member cache
        if let Some(cached_member) = parts.extensions.get::<ResolvedMember<M>>() {
            let role = parts
                .extensions
                .get::<ResolvedRole>()
                .map(|r| r.0.clone())
                .unwrap_or_default();
            return Ok(Member {
                tenant,
                inner: (*cached_member.0).clone(),
                role,
            });
        }

        // 3. Get user_id from session
        let session = SessionManager::from_request_parts(parts, state)
            .await
            .map_err(|_| Error::internal("Member<T, M> requires session middleware"))?;
        let user_id = session
            .user_id()
            .await
            .ok_or_else(|| Error::from(HttpError::Unauthorized))?;

        // 4. Look up member
        let provider = state
            .services
            .get::<MemberProviderService<M, T>>()
            .ok_or_else(|| {
                Error::internal(format!(
                    "MemberProviderService<{}, {}> not registered",
                    std::any::type_name::<M>(),
                    std::any::type_name::<T>()
                ))
            })?;

        let member = provider
            .find_member(&user_id, tenant.tenant_id())
            .await?
            .ok_or_else(|| Error::from(HttpError::Forbidden))?;

        let role = provider.role(&member).to_string();

        // 5. Cache
        parts
            .extensions
            .insert(ResolvedMember(Arc::new(member.clone())));
        parts.extensions.insert(ResolvedRole(role.clone()));

        Ok(Member {
            tenant,
            inner: member,
            role,
        })
    }
}
```

**Step 3: Update lib.rs exports**

Add `Member` to exports in `modo-tenant/src/lib.rs`:

```rust
pub use extractor::{Member, OptionalTenant, Tenant};
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p modo-tenant`
Expected: all tests pass

**Step 5: Commit**

```bash
git add modo-tenant/src/
git commit -m "$(cat <<'EOF'
feat(modo-tenant): add Member<T, M> extractor with tenant + auth + membership resolution
EOF
)"
```

---

## Task 6: `TenantContext<T, M, U>` extractor

**Files:**
- Modify: `modo-tenant/src/extractor.rs`
- Modify: `modo-tenant/src/lib.rs`
- Modify: `modo-tenant/Cargo.toml` (add `modo-auth` dependency)

**Step 1: Add `modo-auth` dependency**

Add to `modo-tenant/Cargo.toml` dependencies:

```toml
modo-auth = { path = "../modo-auth" }
```

**Step 2: Write test for `TenantContext` struct API**

Append to tests in `modo-tenant/src/extractor.rs`:

```rust
    #[test]
    fn tenant_context_accessors() {
        let ctx = TenantContext::<TestTenant, TestMember, TestUser> {
            tenant: TestTenant { id: "t-1".to_string(), name: "Acme".to_string() },
            member: TestMember { user_id: "u-1".to_string(), tenant_id: "t-1".to_string(), role: "admin".to_string() },
            user: TestUser { id: "u-1".to_string(), name: "Alice".to_string() },
            tenants: vec![
                TestTenant { id: "t-1".to_string(), name: "Acme".to_string() },
            ],
            role: "admin".to_string(),
        };

        assert_eq!(ctx.tenant().tenant_id(), "t-1");
        assert_eq!(ctx.member().user_id, "u-1");
        assert_eq!(ctx.user().name, "Alice");
        assert_eq!(ctx.tenants().len(), 1);
        assert_eq!(ctx.role(), "admin");
    }

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct TestUser {
        id: String,
        name: String,
    }
```

**Step 3: Implement `TenantContext<T, M, U>`**

Add to `modo-tenant/src/extractor.rs`:

```rust
use crate::cache::ResolvedTenants;
use modo_auth::UserProviderService;

/// Full tenant context — everything needed for authenticated tenant pages.
pub struct TenantContext<
    T: HasTenantId + Clone + Send + Sync + 'static,
    M: Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
> {
    tenant: T,
    member: M,
    user: U,
    tenants: Vec<T>,
    role: String,
}

impl<T: HasTenantId + Clone + Send + Sync, M: Clone + Send + Sync, U: Clone + Send + Sync> Clone
    for TenantContext<T, M, U>
{
    fn clone(&self) -> Self {
        Self {
            tenant: self.tenant.clone(),
            member: self.member.clone(),
            user: self.user.clone(),
            tenants: self.tenants.clone(),
            role: self.role.clone(),
        }
    }
}

impl<T: HasTenantId + Clone + Send + Sync, M: Clone + Send + Sync, U: Clone + Send + Sync>
    TenantContext<T, M, U>
{
    pub fn tenant(&self) -> &T {
        &self.tenant
    }

    pub fn member(&self) -> &M {
        &self.member
    }

    pub fn user(&self) -> &U {
        &self.user
    }

    pub fn tenants(&self) -> &[T] {
        &self.tenants
    }

    pub fn role(&self) -> &str {
        &self.role
    }
}

impl<T, M, U> FromRequestParts<AppState> for TenantContext<T, M, U>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
    U: Clone + Send + Sync + 'static,
{
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Resolve member (which resolves tenant internally)
        let member_ext = Member::<T, M>::from_request_parts(parts, state).await?;

        // Load user
        let session = SessionManager::from_request_parts(parts, state)
            .await
            .map_err(|_| Error::internal("TenantContext requires session middleware"))?;
        let user_id = session
            .user_id()
            .await
            .ok_or_else(|| Error::from(HttpError::Unauthorized))?;

        let user_provider = state
            .services
            .get::<UserProviderService<U>>()
            .ok_or_else(|| {
                Error::internal(format!(
                    "UserProviderService<{}> not registered",
                    std::any::type_name::<U>()
                ))
            })?;
        let user = user_provider
            .find_by_id(&user_id)
            .await?
            .ok_or_else(|| Error::from(HttpError::Unauthorized))?;

        // Load tenants list (cached or fresh)
        let tenants = if let Some(cached) = parts.extensions.get::<ResolvedTenants<T>>() {
            (*cached.0).clone()
        } else {
            let provider = state
                .services
                .get::<MemberProviderService<M, T>>()
                .ok_or_else(|| Error::internal("MemberProviderService not registered"))?;
            let list = provider.list_tenants(&user_id).await?;
            parts
                .extensions
                .insert(ResolvedTenants(Arc::new(list.clone())));
            list
        };

        Ok(TenantContext {
            tenant: member_ext.tenant.clone(),
            member: member_ext.into_inner(),
            user,
            tenants,
            role: parts
                .extensions
                .get::<ResolvedRole>()
                .map(|r| r.0.clone())
                .unwrap_or_default(),
        })
    }
}
```

**Step 4: Update lib.rs exports**

```rust
pub use extractor::{Member, OptionalTenant, Tenant, TenantContext};
```

**Step 5: Run tests**

Run: `cargo test -p modo-tenant`
Expected: all tests pass

**Step 6: Commit**

```bash
git add modo-tenant/
git commit -m "$(cat <<'EOF'
feat(modo-tenant): add TenantContext<T, M, U> composite extractor
EOF
)"
```

---

## Task 7: Built-in resolvers (subdomain, header, path prefix)

**Files:**
- Create: `modo-tenant/src/resolvers/mod.rs`
- Create: `modo-tenant/src/resolvers/subdomain.rs`
- Create: `modo-tenant/src/resolvers/header.rs`
- Create: `modo-tenant/src/resolvers/path_prefix.rs`
- Modify: `modo-tenant/src/lib.rs`

**Step 1: Write tests for `SubdomainResolver`**

In `modo-tenant/src/resolvers/subdomain.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use modo::axum::http::Request;

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct T {
        id: String,
    }
    impl crate::HasTenantId for T {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    fn parts(host: &str) -> Parts {
        Request::builder()
            .header("host", host)
            .body(())
            .unwrap()
            .into_parts()
            .0
    }

    #[tokio::test]
    async fn extracts_subdomain() {
        let resolver = SubdomainResolver::new("myapp.com", |slug| async move {
            Ok(Some(T { id: slug }))
        });
        let p = parts("acme.myapp.com");
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(result, Some(T { id: "acme".to_string() }));
    }

    #[tokio::test]
    async fn returns_none_for_bare_domain() {
        let resolver = SubdomainResolver::new("myapp.com", |slug| async move {
            Ok(Some(T { id: slug }))
        });
        let p = parts("myapp.com");
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn returns_none_for_www() {
        let resolver = SubdomainResolver::new("myapp.com", |slug| async move {
            Ok(Some(T { id: slug }))
        });
        let p = parts("www.myapp.com");
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn returns_none_when_no_host() {
        let resolver = SubdomainResolver::new("myapp.com", |slug| async move {
            Ok(Some(T { id: slug }))
        });
        let p = Request::builder().body(()).unwrap().into_parts().0;
        let result = crate::TenantResolver::resolve(&resolver, &p).await.unwrap();
        assert_eq!(result, None);
    }
}
```

**Step 2: Implement `SubdomainResolver`**

```rust
use crate::{HasTenantId, TenantResolver};
use modo::axum::http::request::Parts;
use std::future::Future;
use std::marker::PhantomData;

pub struct SubdomainResolver<T, F> {
    base_domain: String,
    lookup: F,
    _phantom: PhantomData<T>,
}

impl<T, F, Fut> SubdomainResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
{
    pub fn new(base_domain: impl Into<String>, lookup: F) -> Self {
        Self {
            base_domain: base_domain.into(),
            lookup,
            _phantom: PhantomData,
        }
    }
}

impl<T, F, Fut> TenantResolver for SubdomainResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
{
    type Tenant = T;

    async fn resolve(&self, parts: &Parts) -> Result<Option<T>, modo::Error> {
        let host = match parts.headers.get("host").and_then(|v| v.to_str().ok()) {
            Some(h) => h.split(':').next().unwrap_or(h),
            None => return Ok(None),
        };

        let subdomain = host.strip_suffix(&format!(".{}", self.base_domain));
        match subdomain {
            Some(sub) if !sub.is_empty() && sub != "www" => (self.lookup)(sub.to_string()).await,
            _ => Ok(None),
        }
    }
}
```

**Step 3: Write tests and implement `HeaderResolver`**

In `modo-tenant/src/resolvers/header.rs`:

```rust
use crate::{HasTenantId, TenantResolver};
use modo::axum::http::request::Parts;
use std::future::Future;
use std::marker::PhantomData;

pub struct HeaderResolver<T, F> {
    header_name: String,
    lookup: F,
    _phantom: PhantomData<T>,
}

impl<T, F, Fut> HeaderResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
{
    pub fn new(header_name: impl Into<String>, lookup: F) -> Self {
        Self {
            header_name: header_name.into(),
            lookup,
            _phantom: PhantomData,
        }
    }
}

impl<T, F, Fut> TenantResolver for HeaderResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
{
    type Tenant = T;

    async fn resolve(&self, parts: &Parts) -> Result<Option<T>, modo::Error> {
        let value = match parts
            .headers
            .get(&self.header_name)
            .and_then(|v| v.to_str().ok())
        {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => return Ok(None),
        };
        (self.lookup)(value).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modo::axum::http::Request;

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct T {
        id: String,
    }
    impl crate::HasTenantId for T {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    #[tokio::test]
    async fn reads_header() {
        let resolver = HeaderResolver::new("x-tenant-id", |id| async move {
            Ok(Some(T { id }))
        });
        let parts = Request::builder()
            .header("x-tenant-id", "acme")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let result = crate::TenantResolver::resolve(&resolver, &parts).await.unwrap();
        assert_eq!(result, Some(T { id: "acme".to_string() }));
    }

    #[tokio::test]
    async fn returns_none_without_header() {
        let resolver = HeaderResolver::new("x-tenant-id", |id| async move {
            Ok(Some(T { id }))
        });
        let parts = Request::builder().body(()).unwrap().into_parts().0;
        let result = crate::TenantResolver::resolve(&resolver, &parts).await.unwrap();
        assert_eq!(result, None);
    }
}
```

**Step 4: Write tests and implement `PathPrefixResolver`**

In `modo-tenant/src/resolvers/path_prefix.rs`:

```rust
use crate::{HasTenantId, TenantResolver};
use modo::axum::http::request::Parts;
use modo::axum::http::Uri;
use std::future::Future;
use std::marker::PhantomData;

pub struct PathPrefixResolver<T, F> {
    lookup: F,
    _phantom: PhantomData<T>,
}

impl<T, F, Fut> PathPrefixResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
{
    pub fn new(lookup: F) -> Self {
        Self {
            lookup,
            _phantom: PhantomData,
        }
    }
}

impl<T, F, Fut> TenantResolver for PathPrefixResolver<T, F>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>, modo::Error>> + Send,
{
    type Tenant = T;

    async fn resolve(&self, parts: &Parts) -> Result<Option<T>, modo::Error> {
        let path = parts.uri.path();
        let mut segments = path.splitn(3, '/').filter(|s| !s.is_empty());

        let identifier = match segments.next() {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => return Ok(None),
        };

        (self.lookup)(identifier).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modo::axum::http::Request;

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    struct T {
        id: String,
    }
    impl crate::HasTenantId for T {
        fn tenant_id(&self) -> &str {
            &self.id
        }
    }

    #[tokio::test]
    async fn extracts_first_segment() {
        let resolver = PathPrefixResolver::new(|slug| async move {
            Ok(Some(T { id: slug }))
        });
        let parts = Request::builder()
            .uri("/acme/dashboard")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let result = crate::TenantResolver::resolve(&resolver, &parts).await.unwrap();
        assert_eq!(result, Some(T { id: "acme".to_string() }));
    }

    #[tokio::test]
    async fn returns_none_for_root() {
        let resolver = PathPrefixResolver::new(|slug| async move {
            Ok(Some(T { id: slug }))
        });
        let parts = Request::builder()
            .uri("/")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let result = crate::TenantResolver::resolve(&resolver, &parts).await.unwrap();
        assert_eq!(result, None);
    }
}
```

**Step 5: Create `modo-tenant/src/resolvers/mod.rs`**

```rust
pub mod header;
pub mod path_prefix;
pub mod subdomain;

pub use header::HeaderResolver;
pub use path_prefix::PathPrefixResolver;
pub use subdomain::SubdomainResolver;
```

**Step 6: Update `modo-tenant/src/lib.rs`**

Add `pub mod resolvers;` and re-export:

```rust
pub mod cache;
pub mod extractor;
pub mod member;
pub mod resolver;
pub mod resolvers;

pub use extractor::{Member, OptionalTenant, Tenant, TenantContext};
pub use member::{MemberProvider, MemberProviderService};
pub use resolver::{HasTenantId, TenantResolver, TenantResolverService};
pub use resolvers::{HeaderResolver, PathPrefixResolver, SubdomainResolver};
```

**Step 7: Run tests**

Run: `cargo test -p modo-tenant`
Expected: all tests pass

**Step 8: Commit**

```bash
git add modo-tenant/src/
git commit -m "$(cat <<'EOF'
feat(modo-tenant): add built-in resolvers — subdomain, header, path prefix
EOF
)"
```

---

## Task 8: Role guard macros (`#[allow_roles]`, `#[deny_roles]`)

**Files:**
- Create: `modo-tenant/src/guard.rs`
- Modify: `modo-tenant-macros/src/lib.rs`
- Create: `modo-tenant-macros/src/roles.rs`
- Modify: `modo-tenant/src/lib.rs`
- Modify: `modo-tenant/Cargo.toml` (add `modo-tenant-macros` dependency)

**Step 1: Implement guard functions in `modo-tenant/src/guard.rs`**

```rust
use modo::{Error, HttpError};

/// Check that the resolved role is in the allowed list.
pub fn check_allowed(role: &str, allowed: &[&str]) -> Result<(), Error> {
    if allowed.contains(&role) {
        Ok(())
    } else {
        Err(HttpError::Forbidden.into())
    }
}

/// Check that the resolved role is NOT in the denied list.
pub fn check_denied(role: &str, denied: &[&str]) -> Result<(), Error> {
    if denied.contains(&role) {
        Err(HttpError::Forbidden.into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_passes_matching_role() {
        assert!(check_allowed("admin", &["admin", "owner"]).is_ok());
    }

    #[test]
    fn allowed_rejects_non_matching_role() {
        let err = check_allowed("viewer", &["admin", "owner"]).unwrap_err();
        assert_eq!(
            err.status_code(),
            modo::axum::http::StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn denied_blocks_matching_role() {
        let err = check_denied("viewer", &["viewer"]).unwrap_err();
        assert_eq!(
            err.status_code(),
            modo::axum::http::StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn denied_passes_non_matching_role() {
        assert!(check_denied("admin", &["viewer"]).is_ok());
    }
}
```

**Step 2: Implement the `RoleResolver` type-erased service**

Add to `modo-tenant/src/guard.rs`:

```rust
use crate::cache::{ResolvedRole, ResolvedTenant, ResolvedMember};
use crate::member::MemberProviderService;
use crate::resolver::TenantResolverService;
use crate::HasTenantId;
use modo::app::AppState;
use modo::axum::http::request::Parts;
use modo_session::SessionManager;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type ResolveRoleFn = Arc<
    dyn Fn(
            &mut Parts,
            &AppState,
        ) -> Pin<Box<dyn Future<Output = Result<String, Error>> + Send + '_>>
        + Send
        + Sync,
>;

/// Type-erased role resolver. Registered automatically when both
/// TenantResolverService and MemberProviderService are present.
pub struct RoleResolver {
    resolve_fn: ResolveRoleFn,
}

impl RoleResolver {
    /// Create a RoleResolver that captures the generic types T and M.
    pub fn new<T, M>(
        tenant_svc: TenantResolverService<T>,
        member_svc: MemberProviderService<M, T>,
    ) -> Self
    where
        T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
        M: Clone + Send + Sync + serde::Serialize + 'static,
    {
        let resolve_fn: ResolveRoleFn = Arc::new(move |parts, _state| {
            let tenant_svc = tenant_svc.clone();
            let member_svc = member_svc.clone();
            Box::pin(async move {
                // Check cache first
                if let Some(cached) = parts.extensions.get::<ResolvedRole>() {
                    return Ok(cached.0.clone());
                }

                // Resolve tenant
                let tenant = if let Some(cached) = parts.extensions.get::<ResolvedTenant<T>>() {
                    (*cached.0).clone()
                } else {
                    let t = tenant_svc
                        .resolve(parts)
                        .await?
                        .ok_or_else(|| Error::from(HttpError::NotFound))?;
                    parts
                        .extensions
                        .insert(ResolvedTenant(Arc::new(t.clone())));
                    t
                };

                // Get user_id from session extensions
                let session_state = parts
                    .extensions
                    .get::<Arc<modo_session::middleware::SessionManagerState>>()
                    .cloned()
                    .ok_or_else(|| Error::internal("Role guard requires session middleware"))?;
                let user_id = {
                    let current = session_state.current_session.lock().await;
                    current
                        .as_ref()
                        .map(|s| s.user_id.clone())
                        .ok_or_else(|| Error::from(HttpError::Unauthorized))?
                };

                // Resolve member
                let member = member_svc
                    .find_member(&user_id, tenant.tenant_id())
                    .await?
                    .ok_or_else(|| Error::from(HttpError::Forbidden))?;

                let role = member_svc.role(&member).to_string();

                // Cache
                parts
                    .extensions
                    .insert(ResolvedMember(Arc::new(member)));
                parts.extensions.insert(ResolvedRole(role.clone()));

                Ok(role)
            })
        });

        Self { resolve_fn }
    }

    pub async fn resolve(&self, parts: &mut Parts, state: &AppState) -> Result<String, Error> {
        (self.resolve_fn)(parts, state).await
    }
}
```

Note: The `RoleResolver::new` approach above accesses `SessionManagerState` directly from extensions. This avoids needing `SessionManager::from_request_parts` in a non-extractor context. Check if `modo_session::middleware::SessionManagerState` is public — if not, we may need to add a public helper to `modo-session`. **During implementation, verify this and adjust accordingly** (e.g., use `SessionManager::from_request_parts` if possible, or make the necessary types public in modo-session).

**Step 3: Implement middleware factory functions**

Add to `modo-tenant/src/guard.rs`:

```rust
use modo::axum::body::Body;
use modo::axum::http::Request;
use modo::axum::middleware::Next;
use modo::axum::response::{IntoResponse, Response};

/// Middleware factory: allow only specified roles.
pub fn require_roles(
    roles: &'static [&'static str],
) -> impl Fn(Request<Body>, Next) -> Pin<Box<dyn Future<Output = Response> + Send>> + Clone + Send + Sync {
    move |req: Request<Body>, next: Next| {
        let roles = roles;
        Box::pin(async move {
            let (mut parts, body) = req.into_parts();

            let state = parts
                .extensions
                .get::<AppState>()
                .cloned();

            // Fallback: read RoleResolver from a pre-injected extension
            let resolver = parts.extensions.get::<Arc<RoleResolver>>().cloned();

            match resolver {
                Some(resolver) => {
                    // We need AppState for the resolver but middleware doesn't have it directly.
                    // The role resolver only uses parts + cached extensions.
                    match resolver.resolve(&mut parts, &AppState::default()).await {
                        Ok(role) => {
                            if let Err(e) = check_allowed(&role, roles) {
                                return e.into_response();
                            }
                        }
                        Err(e) => return e.into_response(),
                    }
                }
                None => {
                    return Error::internal("RoleResolver not registered").into_response();
                }
            }

            let req = Request::from_parts(parts, body);
            next.run(req).await
        })
    }
}
```

**Important implementation note:** The middleware pattern above is a sketch. During implementation, you'll need to use `from_fn_with_state` to access `AppState` or inject the `RoleResolver` as an extension in the `TenantContextLayer`. The exact approach will depend on how modo's handler macro generates the middleware wrapper. Look at `modo-macros/src/middleware.rs:52-61` — when `args` are present, it calls `#path(#(#args),*)` as a layer factory. So `require_roles("admin", "owner")` must return something that implements `Layer`. Use `axum::middleware::from_fn_with_state` wrapped in a layer-returning function, or return `axum::middleware::from_fn(closure)` directly.

**Step 4: Implement proc macros**

Write `modo-tenant-macros/src/roles.rs`:

```rust
use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, LitStr, Result, Token};

pub struct RoleList(pub Vec<LitStr>);

impl syn::parse::Parse for RoleList {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let roles = input.parse_terminated(LitStr::parse, Token![,])?;
        Ok(RoleList(roles.into_iter().collect()))
    }
}

pub fn expand_allow_roles(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let roles: RoleList = syn::parse2(attr)?;
    let func: ItemFn = syn::parse2(item)?;
    let role_strs: Vec<&LitStr> = roles.0.iter().collect();

    // Inject #[middleware(modo_tenant::guard::require_roles(&[...]))]
    // by adding the attribute to the function
    Ok(quote! {
        #[middleware(modo_tenant::guard::require_roles(&[#(#role_strs),*]))]
        #func
    })
}

pub fn expand_deny_roles(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let roles: RoleList = syn::parse2(attr)?;
    let func: ItemFn = syn::parse2(item)?;
    let role_strs: Vec<&LitStr> = roles.0.iter().collect();

    Ok(quote! {
        #[middleware(modo_tenant::guard::exclude_roles(&[#(#role_strs),*]))]
        #func
    })
}
```

**Step 5: Update `modo-tenant-macros/src/lib.rs`**

```rust
use proc_macro::TokenStream;

mod roles;

#[proc_macro_attribute]
pub fn allow_roles(attr: TokenStream, item: TokenStream) -> TokenStream {
    roles::expand_allow_roles(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[proc_macro_attribute]
pub fn deny_roles(attr: TokenStream, item: TokenStream) -> TokenStream {
    roles::expand_deny_roles(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
```

**Step 6: Add `modo-tenant-macros` dependency to `modo-tenant/Cargo.toml`**

```toml
modo-tenant-macros = { path = "../modo-tenant-macros" }
```

**Step 7: Re-export macros in `modo-tenant/src/lib.rs`**

```rust
pub use modo_tenant_macros::{allow_roles, deny_roles};
```

**Step 8: Run tests and check compilation**

Run: `cargo test -p modo-tenant && cargo check -p modo-tenant-macros`
Expected: all pass

**Step 9: Commit**

```bash
git add modo-tenant/ modo-tenant-macros/
git commit -m "$(cat <<'EOF'
feat(modo-tenant): add role guard macros and RoleResolver service
EOF
)"
```

---

## Task 9: `TenantContextLayer` for template injection

**Files:**
- Create: `modo-tenant/src/context_layer.rs`
- Modify: `modo-tenant/src/lib.rs`
- Modify: `modo-tenant/Cargo.toml` (add optional `modo-templates` dependency)

**Step 1: Add optional templates dependency**

In `modo-tenant/Cargo.toml`:

```toml
[dependencies]
# ... existing deps ...
modo-templates = { path = "../modo-templates", optional = true }

[features]
default = []
templates = ["dep:modo-templates"]
```

**Step 2: Implement `TenantContextLayer`**

Write `modo-tenant/src/context_layer.rs`:

```rust
#[cfg(feature = "templates")]
use crate::cache::{ResolvedMember, ResolvedRole, ResolvedTenant, ResolvedTenants};
#[cfg(feature = "templates")]
use crate::member::MemberProviderService;
#[cfg(feature = "templates")]
use crate::resolver::TenantResolverService;
#[cfg(feature = "templates")]
use crate::HasTenantId;

#[cfg(feature = "templates")]
use futures_util::future::BoxFuture;
#[cfg(feature = "templates")]
use modo::axum::http::Request;
#[cfg(feature = "templates")]
use modo_templates::TemplateContext;
#[cfg(feature = "templates")]
use std::sync::Arc;
#[cfg(feature = "templates")]
use std::task::{Context, Poll};
#[cfg(feature = "templates")]
use tower::{Layer, Service};

/// Layer that injects tenant, member, tenants, and role into TemplateContext.
/// Graceful: skips if no tenant or no auth.
#[cfg(feature = "templates")]
pub struct TenantContextLayer<T, M>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    tenant_svc: TenantResolverService<T>,
    member_svc: MemberProviderService<M, T>,
}

#[cfg(feature = "templates")]
impl<T, M> Clone for TenantContextLayer<T, M>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    fn clone(&self) -> Self {
        Self {
            tenant_svc: self.tenant_svc.clone(),
            member_svc: self.member_svc.clone(),
        }
    }
}

#[cfg(feature = "templates")]
impl<T, M> TenantContextLayer<T, M>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    pub fn new(
        tenant_svc: TenantResolverService<T>,
        member_svc: MemberProviderService<M, T>,
    ) -> Self {
        Self {
            tenant_svc,
            member_svc,
        }
    }
}

#[cfg(feature = "templates")]
impl<S, T, M> Layer<S> for TenantContextLayer<T, M>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    type Service = TenantContextMiddleware<S, T, M>;

    fn layer(&self, inner: S) -> Self::Service {
        TenantContextMiddleware {
            inner,
            tenant_svc: self.tenant_svc.clone(),
            member_svc: self.member_svc.clone(),
        }
    }
}

#[cfg(feature = "templates")]
#[derive(Clone)]
pub struct TenantContextMiddleware<S, T, M>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    inner: S,
    tenant_svc: TenantResolverService<T>,
    member_svc: MemberProviderService<M, T>,
}

#[cfg(feature = "templates")]
impl<S, ReqBody, ResBody, T, M> Service<Request<ReqBody>>
    for TenantContextMiddleware<S, T, M>
where
    S: Service<Request<ReqBody>, Response = modo::axum::http::Response<ResBody>>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let mut inner = self.inner.clone();
        let tenant_svc = self.tenant_svc.clone();
        let member_svc = self.member_svc.clone();

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            // Resolve tenant (cached or fresh)
            let tenant: Option<T> =
                if let Some(cached) = parts.extensions.get::<ResolvedTenant<T>>() {
                    Some((*cached.0).clone())
                } else {
                    match tenant_svc.resolve(&parts).await {
                        Ok(Some(t)) => {
                            parts
                                .extensions
                                .insert(ResolvedTenant(Arc::new(t.clone())));
                            Some(t)
                        }
                        _ => None,
                    }
                };

            // Inject tenant into template context
            if let Some(ref t) = tenant {
                if let Some(ctx) = parts.extensions.get_mut::<TemplateContext>() {
                    ctx.insert(
                        "tenant",
                        minijinja::Value::from_serialize(t),
                    );
                }
            }

            // If user is authenticated and tenant is resolved, load member + tenants
            if let Some(ref tenant) = tenant {
                // Try to get user_id from session extensions
                let user_id = {
                    use modo_session::middleware::SessionManagerState;
                    parts
                        .extensions
                        .get::<Arc<SessionManagerState>>()
                        .and_then(|state| {
                            // Use try_lock to avoid blocking; skip if locked
                            state.current_session.try_lock().ok().and_then(|guard| {
                                guard.as_ref().map(|s| s.user_id.clone())
                            })
                        })
                };

                if let Some(user_id) = user_id {
                    // Load member
                    if let Ok(Some(member)) = member_svc
                        .find_member(&user_id, tenant.tenant_id())
                        .await
                    {
                        let role = member_svc.role(&member).to_string();

                        if let Some(ctx) = parts.extensions.get_mut::<TemplateContext>() {
                            ctx.insert(
                                "member",
                                minijinja::Value::from_serialize(&member),
                            );
                            ctx.insert("role", role.clone());
                        }

                        parts
                            .extensions
                            .insert(ResolvedMember(Arc::new(member)));
                        parts.extensions.insert(ResolvedRole(role));
                    }

                    // Load tenants list
                    if let Ok(tenants) = member_svc.list_tenants(&user_id).await {
                        if let Some(ctx) = parts.extensions.get_mut::<TemplateContext>() {
                            ctx.insert(
                                "tenants",
                                minijinja::Value::from_serialize(&tenants),
                            );
                        }
                        parts
                            .extensions
                            .insert(ResolvedTenants(Arc::new(tenants)));
                    }
                }
            }

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}
```

**Important note:** The `SessionManagerState` access pattern above may need adjustment. During implementation, check if `modo_session::middleware::SessionManagerState` and its `current_session` field are public. If not, you'll need to either:
1. Make them `pub(crate)` → `pub` in modo-session, or
2. Use a different approach (e.g., a helper function in modo-session that reads user_id from extensions without going through the full `SessionManager` extractor)

**Step 3: Update `modo-tenant/src/lib.rs`**

```rust
#[cfg(feature = "templates")]
pub mod context_layer;

#[cfg(feature = "templates")]
pub use context_layer::TenantContextLayer;
```

**Step 4: Add `futures-util` and `minijinja` dependencies**

In `modo-tenant/Cargo.toml`:

```toml
futures-util = "0.3"
minijinja = "2"
```

**Step 5: Run check**

Run: `cargo check -p modo-tenant --features templates`
Expected: compiles

**Step 6: Commit**

```bash
git add modo-tenant/
git commit -m "$(cat <<'EOF'
feat(modo-tenant): add TenantContextLayer for automatic template variable injection
EOF
)"
```

---

## Task 10: `UserContextLayer` in `modo-auth`

**Files:**
- Create: `modo-auth/src/context_layer.rs`
- Modify: `modo-auth/src/lib.rs`
- Modify: `modo-auth/Cargo.toml`

**Step 1: Add optional dependencies**

In `modo-auth/Cargo.toml`:

```toml
[dependencies]
# ... existing deps ...
modo-templates = { path = "../modo-templates", optional = true }
futures-util = "0.3"
minijinja = { version = "2", optional = true }

[features]
default = []
templates = ["dep:modo-templates", "dep:minijinja"]
```

**Step 2: Implement `UserContextLayer`**

Write `modo-auth/src/context_layer.rs`:

```rust
#[cfg(feature = "templates")]
use crate::provider::UserProviderService;

#[cfg(feature = "templates")]
use futures_util::future::BoxFuture;
#[cfg(feature = "templates")]
use modo::axum::http::Request;
#[cfg(feature = "templates")]
use modo_templates::TemplateContext;
#[cfg(feature = "templates")]
use std::sync::Arc;
#[cfg(feature = "templates")]
use std::task::{Context, Poll};
#[cfg(feature = "templates")]
use tower::{Layer, Service};

/// Cached resolved user in request extensions.
#[cfg(feature = "templates")]
pub struct ResolvedUser<U>(pub Arc<U>);

#[cfg(feature = "templates")]
impl<U> Clone for ResolvedUser<U> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Layer that injects the authenticated user into TemplateContext.
/// Graceful: injects null if not authenticated.
#[cfg(feature = "templates")]
pub struct UserContextLayer<U>
where
    U: Clone + Send + Sync + serde::Serialize + 'static,
{
    user_svc: UserProviderService<U>,
}

#[cfg(feature = "templates")]
impl<U> Clone for UserContextLayer<U>
where
    U: Clone + Send + Sync + serde::Serialize + 'static,
{
    fn clone(&self) -> Self {
        Self {
            user_svc: self.user_svc.clone(),
        }
    }
}

#[cfg(feature = "templates")]
impl<U> UserContextLayer<U>
where
    U: Clone + Send + Sync + serde::Serialize + 'static,
{
    pub fn new(user_svc: UserProviderService<U>) -> Self {
        Self { user_svc }
    }
}

#[cfg(feature = "templates")]
impl<S, U> Layer<S> for UserContextLayer<U>
where
    U: Clone + Send + Sync + serde::Serialize + 'static,
{
    type Service = UserContextMiddleware<S, U>;

    fn layer(&self, inner: S) -> Self::Service {
        UserContextMiddleware {
            inner,
            user_svc: self.user_svc.clone(),
        }
    }
}

#[cfg(feature = "templates")]
#[derive(Clone)]
pub struct UserContextMiddleware<S, U>
where
    U: Clone + Send + Sync + serde::Serialize + 'static,
{
    inner: S,
    user_svc: UserProviderService<U>,
}

#[cfg(feature = "templates")]
impl<S, ReqBody, ResBody, U> Service<Request<ReqBody>> for UserContextMiddleware<S, U>
where
    S: Service<Request<ReqBody>, Response = modo::axum::http::Response<ResBody>>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
    U: Clone + Send + Sync + serde::Serialize + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let mut inner = self.inner.clone();
        let user_svc = self.user_svc.clone();

        Box::pin(async move {
            let (mut parts, body) = request.into_parts();

            // Try to get user_id from session extensions
            let user_id = {
                use modo_session::middleware::SessionManagerState;
                parts
                    .extensions
                    .get::<Arc<SessionManagerState>>()
                    .and_then(|state| {
                        state
                            .current_session
                            .try_lock()
                            .ok()
                            .and_then(|guard| guard.as_ref().map(|s| s.user_id.clone()))
                    })
            };

            if let Some(user_id) = user_id {
                if let Ok(Some(user)) = user_svc.find_by_id(&user_id).await {
                    if let Some(ctx) = parts.extensions.get_mut::<TemplateContext>() {
                        ctx.insert("user", minijinja::Value::from_serialize(&user));
                    }
                    parts
                        .extensions
                        .insert(ResolvedUser(Arc::new(user)));
                }
            }

            let request = Request::from_parts(parts, body);
            inner.call(request).await
        })
    }
}
```

**Step 3: Update `modo-auth/src/lib.rs`**

```rust
pub mod context_layer;
pub mod extractor;
pub mod password;
pub mod provider;

pub use extractor::{Auth, OptionalAuth};
pub use password::{PasswordConfig, PasswordHasher};
pub use provider::{UserProvider, UserProviderService};

#[cfg(feature = "templates")]
pub use context_layer::{ResolvedUser, UserContextLayer};
```

**Step 4: Run check**

Run: `cargo check -p modo-auth --features templates`
Expected: compiles

**Step 5: Commit**

```bash
git add modo-auth/
git commit -m "$(cat <<'EOF'
feat(modo-auth): add UserContextLayer for automatic template user injection
EOF
)"
```

---

## Task 11: Verify full workspace compiles and all tests pass

**Step 1: Format**

Run: `just fmt`

**Step 2: Check entire workspace**

Run: `just check`
Expected: fmt, lint, and test all pass

**Step 3: Fix any issues**

Fix clippy warnings, dead code, import issues, etc.

**Step 4: Commit fixes if any**

```bash
git add -A
git commit -m "$(cat <<'EOF'
fix(modo-tenant): address lint and compilation issues
EOF
)"
```

---

## Task 12: Update CLAUDE.md with new conventions

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Add modo-tenant conventions**

Add to the Architecture section:
```
- `modo-tenant/` — multi-tenancy + RBAC (tenant resolution, membership, role guards)
- `modo-tenant-macros/` — `#[allow_roles()]` / `#[deny_roles()]` proc macros
```

Add to the Conventions section:
```
- Tenant resolver: implement `TenantResolver` trait, use `Tenant<T>` / `OptionalTenant<T>` extractors
- Membership: implement `MemberProvider` trait, use `Member<T, M>` extractor
- Full context: `TenantContext<T, M, U>` for handlers needing tenant + member + user + tenants list
- Role guards: `#[allow_roles("admin", "owner")]` / `#[deny_roles("viewer")]` — no extractor needed
- Built-in resolvers: `SubdomainResolver`, `HeaderResolver`, `PathPrefixResolver`
- Template context: `UserContextLayer` (modo-auth) injects `user`; `TenantContextLayer` (modo-tenant) injects `tenant`, `member`, `tenants`, `role`
- HasTenantId: tenant types must implement `HasTenantId` trait
```

**Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "$(cat <<'EOF'
docs: update CLAUDE.md with modo-tenant conventions
EOF
)"
```
