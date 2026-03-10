use crate::middleware::{
    MiddlewareAttr, MiddlewareList, build_middleware_vec, gen_router_middleware_wrapper,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, Item, ItemMod, LitStr, Result, Token, parse2};

struct ModuleArgs {
    prefix: LitStr,
    middleware: Vec<MiddlewareAttr>,
}

impl Parse for ModuleArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut prefix = None;
        let mut middleware = Vec::new();

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match ident.to_string().as_str() {
                "prefix" => {
                    prefix = Some(input.parse::<LitStr>()?);
                }
                "middleware" => {
                    let content;
                    syn::bracketed!(content in input);
                    let list: MiddlewareList = content.parse()?;
                    middleware = list.0;
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "expected `prefix` or `middleware`",
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let prefix = prefix.ok_or_else(|| input.error("missing `prefix` argument"))?;
        Ok(ModuleArgs { prefix, middleware })
    }
}

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args: ModuleArgs = parse2(attr)?;
    let mut module: ItemMod = parse2(item)?;

    let mod_name = &module.ident;
    let mod_name_str = mod_name.to_string();
    let prefix = &args.prefix;

    // Rewrite inner #[handler(METHOD, "/path")] attrs to include module = "mod_name"
    if let Some((_brace, ref mut items)) = module.content {
        for item in items.iter_mut() {
            if let Item::Fn(func) = item {
                rewrite_handler_attrs(func, &mod_name_str)?;
            }
        }
    }

    // Generate module-level middleware wrapper functions
    let mut wrapper_defs = Vec::new();
    let mut wrapper_names = Vec::new();
    for (i, mw_attr) in args.middleware.iter().enumerate() {
        let (name, def) = gen_router_middleware_wrapper(mod_name, i, mw_attr);
        wrapper_names.push(name);
        wrapper_defs.push(def);
    }

    let middleware_vec =
        build_middleware_vec(&wrapper_names, quote! { modo::router::RouterMiddlewareFn });

    Ok(quote! {
        #module

        #(#wrapper_defs)*

        modo::inventory::submit! {
            modo::router::ModuleRegistration {
                name: #mod_name_str,
                prefix: #prefix,
                middleware: #middleware_vec,
            }
        }
    })
}

/// Rewrite `#[modo::handler(METHOD, "/path")]` or `#[handler(METHOD, "/path")]`
/// to include `module = "mod_name"`.
fn rewrite_handler_attrs(func: &mut syn::ItemFn, module_name: &str) -> Result<()> {
    for attr in func.attrs.iter_mut() {
        let is_handler = attr.path().is_ident("handler")
            || (attr.path().segments.len() == 2
                && attr.path().segments[0].ident == "modo"
                && attr.path().segments[1].ident == "handler");

        if is_handler {
            // Parse existing tokens, append module param
            let tokens = attr.meta.require_list()?.tokens.clone();
            let new_tokens = quote! { #tokens, module = #module_name };
            *attr = syn::parse_quote! { #[handler(#new_tokens)] };
        }
    }
    Ok(())
}
