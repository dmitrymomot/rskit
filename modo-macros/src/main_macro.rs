use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, Pat, Result, parse2};

struct MainAttr {
    static_assets: Option<syn::LitStr>,
}

impl syn::parse::Parse for MainAttr {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        if input.is_empty() {
            return Ok(Self {
                static_assets: None,
            });
        }
        let ident: syn::Ident = input.parse()?;
        if ident != "static_assets" {
            return Err(syn::Error::new_spanned(
                &ident,
                "unknown attribute, expected `static_assets`",
            ));
        }
        input.parse::<syn::Token![=]>()?;
        Ok(Self {
            static_assets: Some(input.parse()?),
        })
    }
}

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let main_attr: MainAttr = parse2(attr)?;
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

    if func.sig.inputs.len() != 2 {
        return Err(syn::Error::new_spanned(
            &func.sig.inputs,
            "#[modo::main] requires exactly two parameters: the AppBuilder and a config type, e.g. async fn main(app: AppBuilder, config: MyConfig)",
        ));
    }

    // Extract the parameter name for the AppBuilder binding
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
        unreachable!() // already validated len == 2
    };

    // Extract the config parameter name and type
    let (config_ident, config_ty) = if let Some(FnArg::Typed(pat_type)) =
        func.sig.inputs.iter().nth(1)
    {
        let ident = if let Pat::Ident(pat_ident) = pat_type.pat.as_ref() {
            pat_ident.ident.clone()
        } else {
            return Err(syn::Error::new_spanned(
                &pat_type.pat,
                "#[modo::main] config parameter must be a simple identifier, e.g. `config: AppConfig`",
            ));
        };
        let ty = pat_type.ty.clone();
        (ident, ty)
    } else {
        unreachable!() // already validated len == 2
    };

    // Generate embed code conditionally
    #[cfg(not(feature = "static-embed"))]
    if main_attr.static_assets.is_some() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "static_assets requires modo's 'static-embed' feature",
        ));
    }

    let static_embed_tokens;
    #[cfg(feature = "static-embed")]
    {
        let folder = main_attr
            .static_assets
            .unwrap_or_else(|| syn::LitStr::new("static/", proc_macro2::Span::call_site()));
        static_embed_tokens = quote! {
            let #app_ident = {
                use ::modo::__internal::rust_embed;
                #[derive(rust_embed::Embed)]
                #[folder = #folder]
                struct __ModoStaticAssets;
                #app_ident.embed_static_files::<__ModoStaticAssets>()
            };
        };
    }
    #[cfg(not(feature = "static-embed"))]
    {
        static_embed_tokens = quote! {};
    }

    let stmts = &func.block.stmts;

    Ok(quote! {
        fn main() {
            modo::__internal::tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime")
                .block_on(async {
                    let stdout_filter = modo::__internal::tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| modo::__internal::tracing_subscriber::EnvFilter::new("info,sqlx::query=warn"));

                    modo::__internal::tracing_subscriber::fmt()
                        .with_env_filter(stdout_filter)
                        .init();

                    let #app_ident = modo::__internal::AppBuilder::new();

                    #static_embed_tokens

                    let __modo_result: std::result::Result<(), Box<dyn std::error::Error>> = {
                        async move {
                            let #config_ident: #config_ty = modo::__internal::load_or_default()?;
                            #(#stmts)*
                        }
                    }.await;

                    if let Err(e) = __modo_result {
                        modo::__internal::tracing::error!("Application error: {e}");
                        std::process::exit(1);
                    }
                });
        }
    })
}
