use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Path, Result, Token};

/// Represents a single middleware attribute, e.g.:
/// - `auth_required` (bare path → wrap with `from_fn()`)
/// - `require_role("admin")` (path + args → call as layer factory)
pub struct MiddlewareAttr {
    pub path: Path,
    pub args: Option<Vec<Expr>>,
}

impl Parse for MiddlewareAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let path: Path = input.parse()?;

        let args = if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            let args = content.parse_terminated(Expr::parse, Token![,])?;
            Some(args.into_iter().collect())
        } else {
            None
        };

        Ok(MiddlewareAttr { path, args })
    }
}

/// Parse multiple middleware attrs from a comma-separated list inside `#[middleware(...)]`.
pub struct MiddlewareList(pub Vec<MiddlewareAttr>);

impl Parse for MiddlewareList {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.parse_terminated(MiddlewareAttr::parse, Token![,])?;
        Ok(MiddlewareList(attrs.into_iter().collect()))
    }
}

/// Generate a middleware wrapper function for a handler-level middleware.
/// Returns (wrapper_fn_ident, wrapper_fn_definition).
pub fn gen_handler_middleware_wrapper(
    handler_name: &syn::Ident,
    index: usize,
    attr: &MiddlewareAttr,
) -> (syn::Ident, TokenStream) {
    let wrapper_name =
        syn::Ident::new(&format!("__mw_{handler_name}_{index}"), handler_name.span());
    let path = &attr.path;

    let layer_expr = match &attr.args {
        None => {
            // Bare function → wrap with from_fn()
            quote! { rskit::axum::middleware::from_fn(#path) }
        }
        Some(args) => {
            // Factory function with args → call directly, returns a Layer
            quote! { #path(#(#args),*) }
        }
    };

    let def = quote! {
        fn #wrapper_name(
            mr: rskit::axum::routing::MethodRouter<rskit::app::AppState>,
        ) -> rskit::axum::routing::MethodRouter<rskit::app::AppState> {
            mr.route_layer(#layer_expr)
        }
    };

    (wrapper_name, def)
}

/// Generate a middleware wrapper function for a module-level (router) middleware.
/// Returns (wrapper_fn_ident, wrapper_fn_definition).
pub fn gen_router_middleware_wrapper(
    module_name: &syn::Ident,
    index: usize,
    attr: &MiddlewareAttr,
) -> (syn::Ident, TokenStream) {
    let wrapper_name = syn::Ident::new(
        &format!("__mw_mod_{module_name}_{index}"),
        module_name.span(),
    );
    let path = &attr.path;

    let layer_expr = match &attr.args {
        None => {
            quote! { rskit::axum::middleware::from_fn(#path) }
        }
        Some(args) => {
            quote! { #path(#(#args),*) }
        }
    };

    let def = quote! {
        fn #wrapper_name(
            router: rskit::axum::Router<rskit::app::AppState>,
        ) -> rskit::axum::Router<rskit::app::AppState> {
            router.layer(#layer_expr)
        }
    };

    (wrapper_name, def)
}
