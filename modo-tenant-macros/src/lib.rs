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
