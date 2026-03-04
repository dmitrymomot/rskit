use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, Pat, Result, parse2};

pub fn expand(_attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let func: ItemFn = parse2(item)?;

    if func.sig.ident != "main" {
        return Err(syn::Error::new_spanned(
            &func.sig.ident,
            "#[rskit::main] can only be applied to a function named `main`",
        ));
    }

    if func.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            func.sig.fn_token,
            "#[rskit::main] requires an async function",
        ));
    }

    if func.sig.inputs.len() > 1 {
        return Err(syn::Error::new_spanned(
            &func.sig.inputs,
            "#[rskit::main] accepts at most one parameter: the AppBuilder",
        ));
    }

    // Extract the parameter name for the AppBuilder binding (default: `app`)
    let app_ident = if let Some(FnArg::Typed(pat_type)) = func.sig.inputs.first() {
        if let Pat::Ident(pat_ident) = pat_type.pat.as_ref() {
            pat_ident.ident.clone()
        } else {
            return Err(syn::Error::new_spanned(
                &pat_type.pat,
                "#[rskit::main] parameter must be a simple identifier, e.g. `app: AppBuilder`",
            ));
        }
    } else {
        syn::Ident::new("app", proc_macro2::Span::call_site())
    };

    let func_body = &func.block;

    Ok(quote! {
        fn main() {
            rskit::tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime")
                .block_on(async {
                    let config = rskit::config::AppConfig::from_env();

                    let stdout_filter = rskit::tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| rskit::tracing_subscriber::EnvFilter::new(&config.log_level));

                    let _sentry_guard = config.sentry_dsn.as_deref().map(|dsn| {
                        rskit::sentry::init((dsn, rskit::sentry::ClientOptions {
                            traces_sample_rate: match config.environment {
                                rskit::config::Environment::Production => 0.1,
                                _ => 1.0,
                            },
                            environment: Some(match config.environment {
                                rskit::config::Environment::Production => "production".into(),
                                rskit::config::Environment::Development => "development".into(),
                                rskit::config::Environment::Test => "test".into(),
                            }),
                            release: rskit::sentry::release_name!(),
                            ..Default::default()
                        }))
                    });

                    if _sentry_guard.is_some() {
                        use rskit::tracing_subscriber::prelude::*;
                        let sentry_filter = rskit::tracing_subscriber::EnvFilter::new(&config.sentry_log_level);
                        rskit::tracing_subscriber::registry()
                            .with(rskit::tracing_subscriber::fmt::layer().with_filter(stdout_filter))
                            .with(rskit::sentry::integrations::tracing::layer().with_filter(sentry_filter))
                            .init();
                        rskit::tracing::info!("Sentry initialized");
                    } else {
                        rskit::tracing_subscriber::fmt()
                            .with_env_filter(stdout_filter)
                            .init();
                    }
                    let #app_ident = rskit::app::AppBuilder::new(config);

                    let __rskit_result: std::result::Result<(), Box<dyn std::error::Error>> = {
                        async move #func_body
                    }.await;

                    if let Err(e) = __rskit_result {
                        rskit::tracing::error!("Application error: {e}");
                        std::process::exit(1);
                    }
                });
        }
    })
}
