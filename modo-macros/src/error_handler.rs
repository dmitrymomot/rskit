use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, Result, parse2};

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    if !attr.is_empty() {
        return Err(syn::Error::new_spanned(
            attr,
            "#[modo::error_handler] takes no arguments",
        ));
    }

    let func: ItemFn = parse2(item)?;

    if func.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            func.sig.fn_token,
            "error handler must be a sync function",
        ));
    }

    let params = &func.sig.inputs;
    if params.len() != 2 {
        return Err(syn::Error::new_spanned(
            params,
            "error handler must have exactly 2 parameters: (modo::Error, &modo::ErrorContext)",
        ));
    }

    let func_name = &func.sig.ident;

    Ok(quote! {
        #func

        modo::__internal::inventory::submit! {
            modo::__internal::ErrorHandlerRegistration {
                handler: #func_name as modo::__internal::ErrorHandlerFn,
            }
        }
    })
}
