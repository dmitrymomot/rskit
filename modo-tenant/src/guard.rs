use crate::HasTenantId;
use crate::cache::{ResolvedMember, ResolvedRole, ResolvedTenant};
use crate::member::MemberProviderService;
use crate::resolver::TenantResolverService;
use modo::{Error, HttpError};
use std::sync::Arc;

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

/// Resolve the current user's role for the current tenant from request extensions.
///
/// Checks cached `ResolvedRole` first, then resolves tenant + member if needed.
/// Requires session middleware and both `TenantResolverService` and `MemberProviderService`
/// to be registered.
pub async fn resolve_role<T, M>(
    extensions: &mut http::Extensions,
    _tenant_svc: &TenantResolverService<T>,
    member_svc: &MemberProviderService<M, T>,
) -> Result<String, Error>
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    // Check cache first
    if let Some(cached) = extensions.get::<ResolvedRole>() {
        return Ok(cached.0.clone());
    }

    // Resolve tenant
    let tenant = if let Some(cached) = extensions.get::<ResolvedTenant<T>>() {
        (*cached.0).clone()
    } else {
        // Build temporary Parts for the resolver
        // We can't call resolve on extensions alone — need request parts.
        // This path should rarely hit since TenantContextLayer/extractors cache the tenant.
        return Err(Error::internal(
            "Role guard requires tenant to be resolved first (use Tenant<T> extractor or TenantContextLayer)",
        ));
    };

    // Get user_id from session
    let user_id = modo_session::user_id_from_extensions(extensions)
        .ok_or_else(|| Error::from(HttpError::Unauthorized))?;

    // Resolve member
    let member = member_svc
        .find_member(&user_id, tenant.tenant_id())
        .await?
        .ok_or_else(|| Error::from(HttpError::Forbidden))?;

    let role = member_svc.role(&member).to_string();

    // Cache
    extensions.insert(ResolvedMember(Arc::new(member)));
    extensions.insert(ResolvedRole(role.clone()));

    Ok(role)
}

/// Middleware function: allow only specified roles.
///
/// Usage with modo handler macro:
/// ```ignore
/// #[modo::handler(GET, "/admin")]
/// #[middleware(modo_tenant::guard::require_roles::<MyTenant, MyMember>(&["admin", "owner"]))]
/// async fn admin_page() -> &'static str { "admin" }
/// ```
pub fn require_roles<T, M>(
    roles: &'static [&'static str],
) -> impl Fn(
    modo::axum::http::Request<modo::axum::body::Body>,
    modo::axum::middleware::Next,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = modo::axum::response::Response> + Send>,
> + Clone
+ Send
+ Sync
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    use modo::axum::response::IntoResponse;

    move |req: modo::axum::http::Request<modo::axum::body::Body>,
          next: modo::axum::middleware::Next| {
        Box::pin(async move {
            let (mut parts, body) = req.into_parts();

            let state = parts
                .extensions
                .get::<modo::app::AppState>()
                .cloned()
                .or_else(|| {
                    // Try to get from the extensions that axum injects
                    None
                });

            // Get services from state or extensions
            let (tenant_svc, member_svc) = if let Some(ref state) = state {
                let t = state.services.get::<TenantResolverService<T>>();
                let m = state.services.get::<MemberProviderService<M, T>>();
                match (t, m) {
                    (Some(t), Some(m)) => (t, m),
                    _ => {
                        return Error::internal("Role guard: services not registered")
                            .into_response();
                    }
                }
            } else {
                return Error::internal("Role guard: AppState not available").into_response();
            };

            match resolve_role::<T, M>(&mut parts.extensions, &tenant_svc, &member_svc).await {
                Ok(role) => {
                    if let Err(e) = check_allowed(&role, roles) {
                        return e.into_response();
                    }
                }
                Err(e) => return e.into_response(),
            }

            let req = modo::axum::http::Request::from_parts(parts, body);
            next.run(req).await
        })
    }
}

/// Middleware function: deny specified roles.
pub fn exclude_roles<T, M>(
    roles: &'static [&'static str],
) -> impl Fn(
    modo::axum::http::Request<modo::axum::body::Body>,
    modo::axum::middleware::Next,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = modo::axum::response::Response> + Send>,
> + Clone
+ Send
+ Sync
where
    T: Clone + Send + Sync + HasTenantId + serde::Serialize + 'static,
    M: Clone + Send + Sync + serde::Serialize + 'static,
{
    use modo::axum::response::IntoResponse;

    move |req: modo::axum::http::Request<modo::axum::body::Body>,
          next: modo::axum::middleware::Next| {
        Box::pin(async move {
            let (mut parts, body) = req.into_parts();

            let state = parts.extensions.get::<modo::app::AppState>().cloned();

            let (tenant_svc, member_svc) = if let Some(ref state) = state {
                let t = state.services.get::<TenantResolverService<T>>();
                let m = state.services.get::<MemberProviderService<M, T>>();
                match (t, m) {
                    (Some(t), Some(m)) => (t, m),
                    _ => {
                        return Error::internal("Role guard: services not registered")
                            .into_response();
                    }
                }
            } else {
                return Error::internal("Role guard: AppState not available").into_response();
            };

            match resolve_role::<T, M>(&mut parts.extensions, &tenant_svc, &member_svc).await {
                Ok(role) => {
                    if let Err(e) = check_denied(&role, roles) {
                        return e.into_response();
                    }
                }
                Err(e) => return e.into_response(),
            }

            let req = modo::axum::http::Request::from_parts(parts, body);
            next.run(req).await
        })
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
        assert_eq!(err.status_code(), modo::axum::http::StatusCode::FORBIDDEN);
    }

    #[test]
    fn denied_blocks_matching_role() {
        let err = check_denied("viewer", &["viewer"]).unwrap_err();
        assert_eq!(err.status_code(), modo::axum::http::StatusCode::FORBIDDEN);
    }

    #[test]
    fn denied_passes_non_matching_role() {
        assert!(check_denied("admin", &["viewer"]).is_ok());
    }
}
