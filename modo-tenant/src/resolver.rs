/// Trait that tenant types must implement to expose their ID.
pub trait HasTenantId {
    fn tenant_id(&self) -> &str;
}
