use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, Result, parse2};

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let func: ItemFn = parse2(item)?;
    let func_name = &func.sig.ident;

    // Parse optional name = "custom_name" attribute
    let filter_name = if attr.is_empty() {
        func_name.to_string()
    } else {
        let name_value: syn::MetaNameValue = parse2(attr)?;
        if name_value.path.is_ident("name") {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &name_value.value
            {
                s.value()
            } else {
                return Err(syn::Error::new_spanned(
                    &name_value.value,
                    "expected string literal for `name`",
                ));
            }
        } else {
            return Err(syn::Error::new_spanned(
                &name_value.path,
                "unknown attribute, expected `name`",
            ));
        }
    };

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
