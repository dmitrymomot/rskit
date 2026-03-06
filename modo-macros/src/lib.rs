use proc_macro::TokenStream;

mod error_handler;
mod handler;
mod main_macro;
mod middleware;
mod module;
mod sanitize;
mod validate;

/// Attribute macro for declaring HTTP handlers with auto-registration.
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    handler::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Entry point macro that wires the entire modo application.
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    main_macro::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Attribute macro for declaring route modules with shared prefix and middleware.
#[proc_macro_attribute]
pub fn module(attr: TokenStream, item: TokenStream) -> TokenStream {
    module::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Attribute macro for registering a custom error handler.
#[proc_macro_attribute]
pub fn error_handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    error_handler::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derive macro for struct field sanitization via `#[sanitize(...)]` attributes.
#[proc_macro_derive(Sanitize, attributes(clean))]
pub fn derive_sanitize(input: TokenStream) -> TokenStream {
    sanitize::expand(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derive macro for struct field validation via `#[validate(...)]` attributes.
#[proc_macro_derive(Validate, attributes(validate))]
pub fn derive_validate(input: TokenStream) -> TokenStream {
    validate::expand(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
