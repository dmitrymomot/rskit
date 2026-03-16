use std::str::FromStr;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, Ident, ItemFn, Lit, LitStr, Pat, Result, Token, Type, parse2};

// ---------------------------------------------------------------------------
// Attribute parsing
// ---------------------------------------------------------------------------

struct JobArgs {
    queue: Option<String>,
    priority: Option<i32>,
    max_attempts: Option<u32>,
    timeout: String,
    cron: Option<String>,
}

impl Default for JobArgs {
    fn default() -> Self {
        Self {
            queue: None,
            priority: None,
            max_attempts: None,
            timeout: "5m".to_string(),
            cron: None,
        }
    }
}

impl syn::parse::Parse for JobArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut args = JobArgs::default();

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            if ident == "queue" {
                let val: LitStr = input.parse()?;
                args.queue = Some(val.value());
            } else if ident == "priority" {
                let val: Lit = input.parse()?;
                args.priority = Some(match val {
                    Lit::Int(i) => i.base10_parse()?,
                    _ => return Err(syn::Error::new_spanned(val, "expected integer")),
                });
            } else if ident == "max_attempts" {
                let val: Lit = input.parse()?;
                args.max_attempts = Some(match val {
                    Lit::Int(i) => i.base10_parse()?,
                    _ => return Err(syn::Error::new_spanned(val, "expected integer")),
                });
            } else if ident == "timeout" {
                let val: LitStr = input.parse()?;
                args.timeout = val.value();
            } else if ident == "cron" {
                let val: LitStr = input.parse()?;
                if let Err(e) = cron::Schedule::from_str(&val.value()) {
                    return Err(syn::Error::new_spanned(
                        &val,
                        format!("invalid cron expression: {e}"),
                    ));
                }
                args.cron = Some(val.value());
            } else {
                return Err(syn::Error::new_spanned(
                    &ident,
                    format!("unknown job attribute: {ident}"),
                ));
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        // Mutual exclusion: cron + queue/priority/max_attempts
        if args.cron.is_some()
            && (args.queue.is_some() || args.priority.is_some() || args.max_attempts.is_some())
        {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "cron jobs cannot have queue, priority, or max_attempts attributes",
            ));
        }

        Ok(args)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn parse_duration_secs(s: &str) -> Result<u64> {
    let s = s.trim();
    let (num_str, mult) = if let Some(n) = s.strip_suffix('h') {
        (n, 3600u64)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60)
    } else if let Some(n) = s.strip_suffix('s') {
        (n, 1)
    } else {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("invalid timeout format: {s}. Use e.g. '30s', '5m', '1h'"),
        ));
    };
    num_str.parse::<u64>().map(|n| n * mult).map_err(|_| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("invalid timeout: {s}"),
        )
    })
}

// ---------------------------------------------------------------------------
// Parameter classification
// ---------------------------------------------------------------------------

enum ParamKind {
    Payload(Type),
    Service(Type),
    Db,
}

fn classify_param(arg: &FnArg) -> Result<Option<ParamKind>> {
    let FnArg::Typed(pat_type) = arg else {
        return Ok(None);
    };

    let ty = &*pat_type.ty;

    // Check for Db(db): Db pattern
    if let Pat::TupleStruct(ps) = &*pat_type.pat
        && let Some(last_seg) = ps.path.segments.last()
        && last_seg.ident == "Db"
    {
        return Ok(Some(ParamKind::Db));
    }

    // Check type path for Service<T> or Db
    if let Type::Path(type_path) = ty
        && let Some(last_seg) = type_path.path.segments.last()
    {
        if last_seg.ident == "Db" {
            return Ok(Some(ParamKind::Db));
        }
        if last_seg.ident == "Service"
            && let syn::PathArguments::AngleBracketed(ref args) = last_seg.arguments
            && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
        {
            return Ok(Some(ParamKind::Service(inner.clone())));
        }
    }

    // Otherwise it's a payload
    Ok(Some(ParamKind::Payload(ty.clone())))
}

// ---------------------------------------------------------------------------
// Code generation
// ---------------------------------------------------------------------------

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args: JobArgs = parse2(attr)?;
    let func: ItemFn = parse2(item)?;

    if func.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            func.sig.fn_token,
            "#[job] handlers must be async functions",
        ));
    }

    let func_name = &func.sig.ident;
    let func_name_str = func_name.to_string();
    let impl_name = format_ident!("__job_{}_impl", func_name);
    let struct_name = format_ident!("{}Job", to_pascal_case(&func_name_str));

    let timeout_secs = parse_duration_secs(&args.timeout)?;
    let queue = args.queue.as_deref().unwrap_or("default");
    let priority = args.priority.unwrap_or(0);
    let max_attempts = args.max_attempts.unwrap_or(3);

    let cron_expr = match &args.cron {
        Some(expr) => quote! { Some(#expr) },
        None => quote! { None },
    };

    // Classify parameters
    let mut payload_type: Option<Type> = None;
    let mut setup_stmts = Vec::new();
    let mut call_args = Vec::new();

    for arg in &func.sig.inputs {
        let FnArg::Typed(pat_type) = arg else {
            continue;
        };

        let param_pat = &pat_type.pat;

        match classify_param(arg)? {
            Some(ParamKind::Payload(ty)) => {
                if payload_type.is_some() {
                    return Err(syn::Error::new_spanned(
                        pat_type,
                        "job functions can only have one payload parameter; \
                         use Service<T> or Db for additional dependencies",
                    ));
                }
                payload_type = Some(ty.clone());
                setup_stmts.push(quote! {
                    let #param_pat: #ty = ctx.payload()?;
                });
                call_args.push(quote! { #param_pat });
            }
            Some(ParamKind::Service(inner_ty)) => {
                // Extract the pattern ident for the call arg
                let ident = extract_pat_ident(param_pat);
                setup_stmts.push(quote! {
                    let #ident = modo_jobs::__internal::modo::extractor::service::Service(ctx.service::<#inner_ty>()?);
                });
                call_args.push(quote! { #ident });
            }
            Some(ParamKind::Db) => {
                setup_stmts.push(quote! {
                    let __db = modo_jobs::__internal::modo_db::extractor::Db(ctx.db()?.clone());
                });
                // Use the original pattern for the call
                call_args.push(quote! { __db });
            }
            None => {}
        }
    }

    // Rename original function
    let mut impl_func = func.clone();
    impl_func.sig.ident = impl_name.clone();
    impl_func.vis = syn::Visibility::Inherited;

    // Generate handler impl with JOB_NAME const
    let handler_impl = quote! {
        pub struct #struct_name;

        impl #struct_name {
            pub const JOB_NAME: &str = #func_name_str;
        }

        impl modo_jobs::__internal::JobHandler for #struct_name {
            async fn run(&self, ctx: modo_jobs::__internal::JobContext) -> Result<(), modo_jobs::__internal::modo::Error> {
                #(#setup_stmts)*
                #impl_name(#(#call_args),*).await
            }
        }
    };

    // Generate enqueue methods (only for non-cron jobs)
    let enqueue_methods = if args.cron.is_none() {
        let (payload_param, payload_arg) = match &payload_type {
            Some(ty) => (quote! { payload: &#ty, }, quote! { payload }),
            None => (quote! {}, quote! { &() }),
        };
        quote! {
            impl #struct_name {
                pub async fn enqueue(
                    queue: &modo_jobs::__internal::JobQueue,
                    #payload_param
                ) -> Result<modo_jobs::__internal::JobId, modo_jobs::__internal::modo::Error> {
                    queue.enqueue(Self::JOB_NAME, #payload_arg).await
                }

                pub async fn enqueue_at(
                    queue: &modo_jobs::__internal::JobQueue,
                    #payload_param
                    run_at: modo_jobs::__internal::chrono::DateTime<modo_jobs::__internal::chrono::Utc>,
                ) -> Result<modo_jobs::__internal::JobId, modo_jobs::__internal::modo::Error> {
                    queue.enqueue_at(Self::JOB_NAME, #payload_arg, run_at).await
                }
            }
        }
    } else {
        quote! {}
    };

    // Registration
    let registration = quote! {
        modo_jobs::__internal::inventory::submit! {
            modo_jobs::__internal::JobRegistration {
                name: #struct_name::JOB_NAME,
                queue: #queue,
                priority: #priority,
                max_attempts: #max_attempts,
                timeout_secs: #timeout_secs,
                cron: #cron_expr,
                handler_factory: || Box::new(#struct_name),
            }
        }
    };

    Ok(quote! {
        #impl_func

        #handler_impl

        #enqueue_methods

        #registration
    })
}

fn extract_pat_ident(pat: &Pat) -> TokenStream {
    match pat {
        Pat::Ident(ident) => {
            let i = &ident.ident;
            quote! { #i }
        }
        Pat::TupleStruct(ts) => {
            // For patterns like Service(svc), extract the inner ident
            if let Some(inner) = ts.elems.first() {
                extract_pat_ident(inner)
            } else {
                let fallback = format_ident!("__param");
                quote! { #fallback }
            }
        }
        _ => {
            let fallback = format_ident!("__param");
            quote! { #fallback }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    #[test]
    fn valid_cron_expression_parses() {
        assert!(cron::Schedule::from_str("0 */5 * * * *").is_ok());
        assert!(cron::Schedule::from_str("0 0 * * * *").is_ok());
        assert!(cron::Schedule::from_str("0 0 0 * * *").is_ok());
    }

    #[test]
    fn invalid_cron_expression_fails() {
        assert!(cron::Schedule::from_str("not a cron").is_err());
        assert!(cron::Schedule::from_str("").is_err());
        assert!(cron::Schedule::from_str("* * *").is_err());
    }
}
