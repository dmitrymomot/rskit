use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, Pat, Result, parse2};

pub fn expand(_attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let func: ItemFn = parse2(item)?;

    if func.sig.ident != "main" {
        return Err(syn::Error::new_spanned(
            &func.sig.ident,
            "#[modo::main] can only be applied to a function named `main`",
        ));
    }

    if func.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            func.sig.fn_token,
            "#[modo::main] requires an async function",
        ));
    }

    if func.sig.inputs.len() > 1 {
        return Err(syn::Error::new_spanned(
            &func.sig.inputs,
            "#[modo::main] accepts at most one parameter: the AppBuilder",
        ));
    }

    // Extract the parameter name for the AppBuilder binding (default: `app`)
    let app_ident = if let Some(FnArg::Typed(pat_type)) = func.sig.inputs.first() {
        if let Pat::Ident(pat_ident) = pat_type.pat.as_ref() {
            pat_ident.ident.clone()
        } else {
            return Err(syn::Error::new_spanned(
                &pat_type.pat,
                "#[modo::main] parameter must be a simple identifier, e.g. `app: AppBuilder`",
            ));
        }
    } else {
        syn::Ident::new("app", proc_macro2::Span::call_site())
    };

    let func_body = &func.block;

    Ok(quote! {
        fn main() {
            modo::tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime")
                .block_on(async {
                    let stdout_filter = modo::tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| modo::tracing_subscriber::EnvFilter::new("info,sqlx::query=warn"));

                    modo::tracing_subscriber::fmt()
                        .with_env_filter(stdout_filter)
                        .init();

                    let #app_ident = modo::app::AppBuilder::new();

                    let __modo_result: std::result::Result<(), Box<dyn std::error::Error>> = {
                        async move #func_body
                    }.await;

                    if let Err(e) = __modo_result {
                        modo::tracing::error!("Application error: {e}");
                        std::process::exit(1);
                    }
                });
        }
    })
}
