use proc_macro::TokenStream;

mod entity;
mod migration;

/// Attribute macro for declaring database entities with auto-registration.
///
/// Usage: `#[modo_db::entity(table = "users")]`
#[proc_macro_attribute]
pub fn entity(attr: TokenStream, item: TokenStream) -> TokenStream {
    entity::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Attribute macro for declaring escape-hatch migrations with auto-registration.
///
/// Usage: `#[modo_db::migration(version = 1, description = "...")]`
#[proc_macro_attribute]
pub fn migration(attr: TokenStream, item: TokenStream) -> TokenStream {
    migration::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
