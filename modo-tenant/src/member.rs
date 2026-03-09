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

    fn role<'a>(&'a self, member: &'a Self::Member) -> &'a str;
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
pub struct MemberProviderService<M: Clone + Send + Sync + 'static, T: Clone + Send + Sync + 'static>
{
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

    pub fn role<'a>(&'a self, member: &'a M) -> &'a str {
        self.inner.role(member)
    }
}

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

        async fn list_tenants(&self, user_id: &str) -> Result<Vec<Self::Tenant>, modo::Error> {
            if user_id == "u-1" {
                Ok(vec![
                    TestTenant {
                        id: "t-1".to_string(),
                        name: "Acme".to_string(),
                    },
                    TestTenant {
                        id: "t-2".to_string(),
                        name: "Beta".to_string(),
                    },
                ])
            } else {
                Ok(vec![])
            }
        }

        fn role<'a>(&'a self, member: &'a Self::Member) -> &'a str {
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
