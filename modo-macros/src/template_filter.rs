use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, Result, parse2};

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let func: ItemFn = parse2(item)?;
    let func_name = &func.sig.ident;

    let filter_name = crate::utils::parse_name_attr(attr, func_name.to_string())?;

    Ok(quote! {
        #[allow(dead_code)] // fn is referenced via inventory::submit! below
        #func

        #[allow(unexpected_cfgs)]
        #[cfg(feature = "templates")]
        ::modo::inventory::submit! {
            ::modo::templates::TemplateFilterEntry {
                name: #filter_name,
                register_fn: |env: &mut ::modo::minijinja::Environment<'static>| {
                    env.add_filter(#filter_name, #func_name);
                },
            }
        }
    })
}
