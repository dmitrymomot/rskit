use crate::middleware::{MiddlewareList, build_middleware_vec, gen_handler_middleware_wrapper};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, Ident, ItemFn, LitStr, Pat, Result, Token, Type, parse2};

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

/// Extract `{name}` path parameters from a route path string.
fn extract_path_params(path: &str) -> Vec<String> {
    let mut params = Vec::new();
    for segment in path.split('/') {
        if let Some(inner) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            let name = inner.strip_prefix('*').unwrap_or(inner);
            if !name.is_empty() {
                params.push(name.to_string());
            }
        }
    }
    params
}

struct PathParamInfo {
    sig_index: usize,
    ident: Ident,
    ty: Type,
}

/// Find function parameters that match path parameter names.
fn collect_path_params(func: &ItemFn, path_params: &[String]) -> Vec<PathParamInfo> {
    let mut matched = Vec::new();
    for (i, arg) in func.sig.inputs.iter().enumerate() {
        if let FnArg::Typed(pat_type) = arg
            && let Pat::Ident(pat_ident) = &*pat_type.pat
        {
            let name = pat_ident.ident.to_string();
            if path_params.contains(&name) {
                matched.push(PathParamInfo {
                    sig_index: i,
                    ident: pat_ident.ident.clone(),
                    ty: (*pat_type.ty).clone(),
                });
            }
        }
    }
    matched
}

/// Generate a deserializable struct for path params and rewrite the function signature.
fn transform_path_params(
    func_name: &Ident,
    func: &mut ItemFn,
    path_params: &[String],
    matched: &[PathParamInfo],
) -> TokenStream {
    let struct_name = format_ident!("__{func_name}PathParams", func_name = func_name);

    // Build struct fields: declared params use their type, undeclared default to String
    let declared: std::collections::HashMap<String, &Type> = matched
        .iter()
        .map(|m| (m.ident.to_string(), &m.ty))
        .collect();

    let struct_fields: Vec<TokenStream> = path_params
        .iter()
        .map(|name| {
            let ident = format_ident!("{}", name);
            if let Some(ty) = declared.get(name.as_str()) {
                quote! { #ident: #ty }
            } else {
                quote! { #ident: String }
            }
        })
        .collect();

    let struct_def = quote! {
        #[allow(non_camel_case_types, dead_code)]
        #[derive(modo::serde::Deserialize)]
        #[serde(crate = "modo::serde")]
        struct #struct_name {
            #(#struct_fields),*
        }
    };

    // Remove matched params from function signature in a single pass
    let remove_indices: std::collections::HashSet<usize> =
        matched.iter().map(|m| m.sig_index).collect();
    func.sig.inputs = func
        .sig
        .inputs
        .iter()
        .enumerate()
        .filter(|(i, _)| !remove_indices.contains(i))
        .map(|(_, arg)| arg.clone())
        .collect();

    // Build destructuring pattern with only declared fields + `..`
    let field_idents: Vec<&Ident> = matched.iter().map(|m| &m.ident).collect();
    let path_extractor: syn::FnArg = syn::parse_quote! {
        modo::axum::extract::Path(#struct_name { #(#field_idents),*, .. }):
            modo::axum::extract::Path<#struct_name>
    };

    // Insert the Path extractor at position 0
    func.sig.inputs.insert(0, path_extractor);

    struct_def
}

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args: HandlerArgs = parse2(attr)?;
    let mut func: ItemFn = parse2(item)?;

    let func_name = func.sig.ident.clone();
    let method_ident = &args.method;
    let path = &args.path;

    // Auto-extract path parameters from route path
    let path_params = extract_path_params(&args.path.value());
    let mut path_struct_def = quote! {};
    if !path_params.is_empty() {
        let matched = collect_path_params(&func, &path_params);
        if !matched.is_empty() {
            path_struct_def = transform_path_params(&func_name, &mut func, &path_params, &matched);
        }
    }

    let method_str = method_ident.to_string().to_uppercase();
    let (modo_method, axum_method) = match method_str.as_str() {
        "GET" => (
            quote! { modo::router::Method::GET },
            quote! { modo::axum::routing::get },
        ),
        "POST" => (
            quote! { modo::router::Method::POST },
            quote! { modo::axum::routing::post },
        ),
        "PUT" => (
            quote! { modo::router::Method::PUT },
            quote! { modo::axum::routing::put },
        ),
        "PATCH" => (
            quote! { modo::router::Method::PATCH },
            quote! { modo::axum::routing::patch },
        ),
        "DELETE" => (
            quote! { modo::router::Method::DELETE },
            quote! { modo::axum::routing::delete },
        ),
        "HEAD" => (
            quote! { modo::router::Method::HEAD },
            quote! { modo::axum::routing::head },
        ),
        "OPTIONS" => (
            quote! { modo::router::Method::OPTIONS },
            quote! { modo::axum::routing::options },
        ),
        _ => {
            return Err(syn::Error::new_spanned(
                method_ident,
                format!("unsupported HTTP method: {method_str}"),
            ));
        }
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
        let (name, def) = gen_handler_middleware_wrapper(&func_name, i, mw_attr);
        wrapper_names.push(name);
        wrapper_defs.push(def);
    }

    let middleware_vec =
        build_middleware_vec(&wrapper_names, quote! { modo::router::MiddlewareFn });

    let module_expr = match &args.module {
        Some(m) => quote! { Some(#m) },
        None => quote! { None },
    };

    Ok(quote! {
        #path_struct_def

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
