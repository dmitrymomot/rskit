pub(crate) mod cache;
#[cfg(feature = "templates")]
pub mod context_layer;
pub mod extractor;
pub mod resolver;
pub mod resolvers;

#[cfg(feature = "templates")]
pub use context_layer::TenantContextLayer;
pub use extractor::{OptionalTenant, Tenant};
pub use resolver::{HasTenantId, TenantResolver, TenantResolverService};
pub use resolvers::{HeaderResolver, PathPrefixResolver, SubdomainResolver};
