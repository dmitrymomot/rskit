use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, Ident, ItemFn, Lit, LitStr, Pat, Result, Token, Type, parse2};

// ---------------------------------------------------------------------------
// Attribute parsing
// ---------------------------------------------------------------------------

struct JobArgs {
    queue: String,
    priority: i32,
    max_retries: u32,
    timeout: String,
    cron: Option<String>,
}

impl Default for JobArgs {
    fn default() -> Self {
        Self {
            queue: "default".to_string(),
            priority: 0,
            max_retries: 3,
            timeout: "5m".to_string(),
            cron: None,
        }
    }
}

impl syn::parse::Parse for JobArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut args = JobArgs::default();
        let mut has_queue = false;
        let mut has_priority = false;
        let mut has_max_retries = false;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match ident.to_string().as_str() {
                "queue" => {
                    let val: LitStr = input.parse()?;
                    args.queue = val.value();
                    has_queue = true;
                }
                "priority" => {
                    let val: Lit = input.parse()?;
                    args.priority = match val {
                        Lit::Int(i) => i.base10_parse()?,
                        _ => return Err(syn::Error::new_spanned(val, "expected integer")),
                    };
                    has_priority = true;
                }
                "max_retries" => {
                    let val: Lit = input.parse()?;
                    args.max_retries = match val {
                        Lit::Int(i) => i.base10_parse()?,
                        _ => return Err(syn::Error::new_spanned(val, "expected integer")),
                    };
                    has_max_retries = true;
                }
                "timeout" => {
                    let val: LitStr = input.parse()?;
                    args.timeout = val.value();
                }
                "cron" => {
                    let val: LitStr = input.parse()?;
                    args.cron = Some(val.value());
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        format!("unknown job attribute: {other}"),
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        // Mutual exclusion: cron + queue/priority/max_retries
        if args.cron.is_some() && (has_queue || has_priority || has_max_retries) {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "cron jobs cannot have queue, priority, or max_retries attributes",
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
    if let Some(num) = s.strip_suffix('s') {
        num.parse::<u64>().map_err(|_| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("invalid timeout: {s}"),
            )
        })
    } else if let Some(num) = s.strip_suffix('m') {
        num.parse::<u64>().map(|n| n * 60).map_err(|_| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("invalid timeout: {s}"),
            )
        })
    } else if let Some(num) = s.strip_suffix('h') {
        num.parse::<u64>().map(|n| n * 3600).map_err(|_| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("invalid timeout: {s}"),
            )
        })
    } else {
        Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("invalid timeout format: {s}. Use e.g. '30s', '5m', '1h'"),
        ))
    }
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

    let func_name = func.sig.ident.clone();
    let func_name_str = func_name.to_string();
    let impl_name = format_ident!("__job_{}_impl", func_name);
    let struct_name = format_ident!("{}Job", to_pascal_case(&func_name_str));

    let timeout_secs = parse_duration_secs(&args.timeout)?;
    let queue = &args.queue;
    let priority = args.priority;
    let max_retries = args.max_retries;
    let is_cron = args.cron.is_some();

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
                    let #ident = modo_jobs::modo::extractors::service::Service(ctx.service::<#inner_ty>()?);
                });
                call_args.push(quote! { #ident });
            }
            Some(ParamKind::Db) => {
                setup_stmts.push(quote! {
                    let __db = modo_jobs::modo_db::extractor::Db(ctx.db()?.clone());
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

    // Generate handler impl
    let handler_impl = quote! {
        pub struct #struct_name;

        impl modo_jobs::JobHandler for #struct_name {
            async fn run(&self, ctx: modo_jobs::JobContext) -> Result<(), modo_jobs::modo::error::Error> {
                #(#setup_stmts)*
                #impl_name(#(#call_args),*).await
            }
        }
    };

    // Generate enqueue methods (only for non-cron jobs)
    let enqueue_methods = if !is_cron {
        if let Some(ref payload_ty) = payload_type {
            quote! {
                impl #struct_name {
                    pub async fn enqueue(
                        queue: &modo_jobs::JobQueue,
                        payload: &#payload_ty,
                    ) -> Result<modo_jobs::JobId, modo_jobs::modo::error::Error> {
                        queue.enqueue(#func_name_str, payload).await
                    }

                    pub async fn enqueue_at(
                        queue: &modo_jobs::JobQueue,
                        payload: &#payload_ty,
                        run_at: modo_jobs::chrono::DateTime<modo_jobs::chrono::Utc>,
                    ) -> Result<modo_jobs::JobId, modo_jobs::modo::error::Error> {
                        queue.enqueue_at(#func_name_str, payload, run_at).await
                    }
                }
            }
        } else {
            // No-payload job
            quote! {
                impl #struct_name {
                    pub async fn enqueue(
                        queue: &modo_jobs::JobQueue,
                    ) -> Result<modo_jobs::JobId, modo_jobs::modo::error::Error> {
                        queue.enqueue(#func_name_str, &()).await
                    }

                    pub async fn enqueue_at(
                        queue: &modo_jobs::JobQueue,
                        run_at: modo_jobs::chrono::DateTime<modo_jobs::chrono::Utc>,
                    ) -> Result<modo_jobs::JobId, modo_jobs::modo::error::Error> {
                        queue.enqueue_at(#func_name_str, &(), run_at).await
                    }
                }
            }
        }
    } else {
        quote! {}
    };

    // Registration
    let registration = quote! {
        modo_jobs::inventory::submit! {
            modo_jobs::JobRegistration {
                name: #func_name_str,
                queue: #queue,
                priority: #priority,
                max_retries: #max_retries,
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
