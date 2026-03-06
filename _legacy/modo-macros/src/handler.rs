use crate::middleware::{MiddlewareList, gen_handler_middleware_wrapper};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, ItemFn, LitStr, Result, Token, parse2};

struct HandlerArgs {
    method: Ident,
    path: LitStr,
    module: Option<LitStr>,
}

impl syn::parse::Parse for HandlerArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let method: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
        let path: LitStr = input.parse()?;

        let module = if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            let ident: Ident = input.parse()?;
            if ident != "module" {
                return Err(syn::Error::new_spanned(ident, "expected `module`"));
            }
            input.parse::<Token![=]>()?;
            Some(input.parse::<LitStr>()?)
        } else {
            None
        };

        Ok(HandlerArgs {
            method,
            path,
            module,
        })
    }
}

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args: HandlerArgs = parse2(attr)?;
    let mut func: ItemFn = parse2(item)?;

    let func_name = &func.sig.ident;
    let method_ident = &args.method;
    let path = &args.path;

    let method_str = method_ident.to_string().to_uppercase();
    let modo_method = match method_str.as_str() {
        "GET" => quote! { modo::router::Method::GET },
        "POST" => quote! { modo::router::Method::POST },
        "PUT" => quote! { modo::router::Method::PUT },
        "PATCH" => quote! { modo::router::Method::PATCH },
        "DELETE" => quote! { modo::router::Method::DELETE },
        "HEAD" => quote! { modo::router::Method::HEAD },
        "OPTIONS" => quote! { modo::router::Method::OPTIONS },
        _ => {
            return Err(syn::Error::new_spanned(
                method_ident,
                format!("unsupported HTTP method: {method_str}"),
            ));
        }
    };

    let axum_method = match method_str.as_str() {
        "GET" => quote! { modo::axum::routing::get },
        "POST" => quote! { modo::axum::routing::post },
        "PUT" => quote! { modo::axum::routing::put },
        "PATCH" => quote! { modo::axum::routing::patch },
        "DELETE" => quote! { modo::axum::routing::delete },
        "HEAD" => quote! { modo::axum::routing::head },
        "OPTIONS" => quote! { modo::axum::routing::options },
        _ => unreachable!(),
    };

    // Extract and remove #[middleware(...)] attributes from the function
    let mut middleware_attrs = Vec::new();
    func.attrs.retain(|attr| {
        if attr.path().is_ident("middleware") {
            match attr.parse_args::<MiddlewareList>() {
                Ok(list) => {
                    middleware_attrs.extend(list.0);
                    false // remove from output
                }
                Err(_) => true, // keep — will produce compile error naturally
            }
        } else {
            true
        }
    });

    // Generate middleware wrapper functions and collect their names
    let mut wrapper_defs = Vec::new();
    let mut wrapper_names = Vec::new();
    for (i, mw_attr) in middleware_attrs.iter().enumerate() {
        let (name, def) = gen_handler_middleware_wrapper(func_name, i, mw_attr);
        wrapper_names.push(name);
        wrapper_defs.push(def);
    }

    let middleware_vec = if wrapper_names.is_empty() {
        quote! { vec![] }
    } else {
        quote! { vec![#(#wrapper_names as modo::router::MiddlewareFn),*] }
    };

    let module_expr = match &args.module {
        Some(m) => quote! { Some(#m) },
        None => quote! { None },
    };

    Ok(quote! {
        #func

        #(#wrapper_defs)*

        modo::inventory::submit! {
            modo::router::RouteRegistration {
                method: #modo_method,
                path: #path,
                handler: || #axum_method(#func_name),
                middleware: #middleware_vec,
                module: #module_expr,
            }
        }
    })
}
