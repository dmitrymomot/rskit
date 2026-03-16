use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Path, Result, Token};

/// Represents a single middleware attribute, e.g.:
/// - `auth_required` (bare path -> wrap with `from_fn()`)
/// - `require_role("admin")` (path + args -> call as layer factory)
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

/// Build the layer expression for a middleware attribute.
fn build_layer_expr(attr: &MiddlewareAttr) -> TokenStream {
    let path = &attr.path;
    match &attr.args {
        None => quote! { modo::__internal::axum::from_fn(#path) },
        Some(args) => quote! { #path(#(#args),*) },
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
    let layer_expr = build_layer_expr(attr);

    let def = quote! {
        fn #wrapper_name(
            mr: modo::__internal::axum::MethodRouter<modo::__internal::AppState>,
        ) -> modo::__internal::axum::MethodRouter<modo::__internal::AppState> {
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
    let layer_expr = build_layer_expr(attr);

    let def = quote! {
        fn #wrapper_name(
            router: modo::__internal::axum::Router<modo::__internal::AppState>,
        ) -> modo::__internal::axum::Router<modo::__internal::AppState> {
            router.layer(#layer_expr)
        }
    };

    (wrapper_name, def)
}

/// Build a `vec![...]` expression casting wrapper function names to the given type.
/// Returns `vec![]` when `wrapper_names` is empty.
pub fn build_middleware_vec(wrapper_names: &[syn::Ident], cast_path: TokenStream) -> TokenStream {
    if wrapper_names.is_empty() {
        quote! { vec![] }
    } else {
        quote! { vec![#(#wrapper_names as #cast_path),*] }
    }
}
