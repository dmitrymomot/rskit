use proc_macro::TokenStream;

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

mod context;
mod handler;
mod job;
mod main_macro;
mod middleware;
mod module;

/// Attribute macro for declaring route modules with shared prefix and middleware.
#[proc_macro_attribute]
pub fn module(attr: TokenStream, item: TokenStream) -> TokenStream {
    module::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derive macro for typed template context with `#[base]`, `#[user]`, and `#[session]` fields.
#[proc_macro_attribute]
pub fn context(attr: TokenStream, item: TokenStream) -> TokenStream {
    context::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Attribute macro for declaring background jobs with auto-registration.
#[proc_macro_attribute]
pub fn job(attr: TokenStream, item: TokenStream) -> TokenStream {
    job::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
