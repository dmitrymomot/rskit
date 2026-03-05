use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, Ident, ItemFn, LitInt, LitStr, Pat, Result, Token, Type, parse2};

struct JobArgs {
    queue: String,
    max_retries: u32,
    timeout_secs: u64,
    cron: Option<String>,
}

impl syn::parse::Parse for JobArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut queue = "default".to_string();
        let mut max_retries = 3u32;
        let mut timeout_secs = 300u64;
        let mut cron = None;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match ident.to_string().as_str() {
                "queue" => {
                    let val: LitStr = input.parse()?;
                    queue = val.value();
                }
                "max_retries" => {
                    let val: LitInt = input.parse()?;
                    max_retries = val.base10_parse()?;
                }
                "timeout" => {
                    let val: LitStr = input.parse()?;
                    timeout_secs = parse_duration_str(&val)?;
                }
                "cron" => {
                    let val: LitStr = input.parse()?;
                    cron = Some(val.value());
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

        Ok(JobArgs {
            queue,
            max_retries,
            timeout_secs,
            cron,
        })
    }
}

fn parse_duration_str(lit: &LitStr) -> Result<u64> {
    let s = lit.value();
    let s = s.trim();

    if let Some(num) = s.strip_suffix('s') {
        num.parse::<u64>()
            .map_err(|_| syn::Error::new_spanned(lit, "invalid seconds value"))
    } else if let Some(num) = s.strip_suffix('m') {
        num.parse::<u64>()
            .map(|n| n * 60)
            .map_err(|_| syn::Error::new_spanned(lit, "invalid minutes value"))
    } else if let Some(num) = s.strip_suffix('h') {
        num.parse::<u64>()
            .map(|n| n * 3600)
            .map_err(|_| syn::Error::new_spanned(lit, "invalid hours value"))
    } else {
        Err(syn::Error::new_spanned(
            lit,
            "timeout must end with 's', 'm', or 'h' (e.g., \"30s\", \"5m\", \"1h\")",
        ))
    }
}

/// Convert snake_case to PascalCase and append "Job".
fn to_struct_name(fn_name: &Ident) -> Ident {
    let s = fn_name.to_string();
    let pascal: String = s
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    format_ident!("{pascal}Job")
}

enum ParamKind {
    Payload(Type),
    Service(Type),
    Db,
}

fn classify_param(ty: &Type) -> ParamKind {
    let ty_str = quote!(#ty).to_string();

    // Check for Db extractor
    if ty_str == "Db" || ty_str.ends_with(":: Db") {
        return ParamKind::Db;
    }

    // Check for Service<T>
    if let Type::Path(type_path) = ty
        && let Some(seg) = type_path.path.segments.last()
        && seg.ident == "Service"
        && let syn::PathArguments::AngleBracketed(args) = &seg.arguments
        && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
    {
        return ParamKind::Service(inner.clone());
    }

    // Default: payload type
    ParamKind::Payload(ty.clone())
}

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args: JobArgs = parse2(attr)?;
    let func: ItemFn = parse2(item)?;

    let func_name = &func.sig.ident;
    let func_name_str = func_name.to_string();
    let struct_name = to_struct_name(func_name);
    let impl_fn_name = format_ident!("__job_{}_impl", func_name);

    let queue_str = &args.queue;
    let max_retries = args.max_retries;
    let timeout_secs = args.timeout_secs;
    let cron_expr = match &args.cron {
        Some(c) => quote! { Some(#c) },
        None => quote! { None },
    };

    // Analyze parameters
    let mut payload_type: Option<Type> = None;
    let mut call_args: Vec<TokenStream> = Vec::new();
    let mut setup_stmts: Vec<TokenStream> = Vec::new();

    for arg in func.sig.inputs.iter() {
        let FnArg::Typed(pat_type) = arg else {
            continue;
        };

        let param_name = if let Pat::Ident(pi) = &*pat_type.pat {
            &pi.ident
        } else {
            continue;
        };

        match classify_param(&pat_type.ty) {
            ParamKind::Payload(ty) => {
                payload_type = Some(ty.clone());
                setup_stmts.push(quote! {
                    let #param_name: #ty = ctx.payload()?;
                });
                call_args.push(quote! { #param_name });
            }
            ParamKind::Service(inner_ty) => {
                setup_stmts.push(quote! {
                    let #param_name = modo::extractors::service::Service(ctx.service::<#inner_ty>()?);
                });
                call_args.push(quote! { #param_name });
            }
            ParamKind::Db => {
                setup_stmts.push(quote! {
                    let #param_name = modo::extractors::db::Db(ctx.db()?.clone());
                });
                call_args.push(quote! { #param_name });
            }
        }
    }

    // Generate the enqueue methods only if there's a payload type
    let enqueue_methods = if let Some(ref pt) = payload_type {
        quote! {
            impl #struct_name {
                pub async fn enqueue(payload: &#pt) -> Result<modo::jobs::JobId, modo::error::Error> {
                    modo::jobs::JobQueue::global().enqueue(#func_name_str, payload).await
                }

                pub async fn enqueue_at(
                    payload: &#pt,
                    run_at: modo::chrono::DateTime<modo::chrono::Utc>,
                ) -> Result<modo::jobs::JobId, modo::error::Error> {
                    modo::jobs::JobQueue::global().enqueue_at(#func_name_str, payload, run_at).await
                }
            }
        }
    } else {
        // No payload — enqueue with empty value
        quote! {
            impl #struct_name {
                pub async fn enqueue() -> Result<modo::jobs::JobId, modo::error::Error> {
                    modo::jobs::JobQueue::global().enqueue(#func_name_str, &modo::serde_json::Value::Null).await
                }

                pub async fn enqueue_at(
                    run_at: modo::chrono::DateTime<modo::chrono::Utc>,
                ) -> Result<modo::jobs::JobId, modo::error::Error> {
                    modo::jobs::JobQueue::global().enqueue_at(#func_name_str, &modo::serde_json::Value::Null, run_at).await
                }
            }
        }
    };

    // Rename original function
    let mut impl_func = func.clone();
    impl_func.sig.ident = impl_fn_name.clone();
    // Remove visibility (it's internal)
    impl_func.vis = syn::Visibility::Inherited;

    Ok(quote! {
        #impl_func

        pub struct #struct_name;

        impl modo::jobs::JobHandler for #struct_name {
            async fn run(&self, ctx: modo::jobs::JobContext) -> Result<(), modo::error::Error> {
                #(#setup_stmts)*
                #impl_fn_name(#(#call_args),*).await
            }
        }

        #enqueue_methods

        modo::inventory::submit! {
            modo::jobs::JobRegistration {
                name: #func_name_str,
                queue: #queue_str,
                max_retries: #max_retries,
                timeout: std::time::Duration::from_secs(#timeout_secs),
                cron: #cron_expr,
                handler_factory: || Box::new(#struct_name) as Box<dyn modo::jobs::JobHandlerDyn>,
            }
        }
    })
}
