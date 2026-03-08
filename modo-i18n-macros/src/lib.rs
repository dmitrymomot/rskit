use proc_macro::TokenStream;

/// Translate a key with optional named variables.
///
/// Usage:
/// - `t!(i18n, "key")`
/// - `t!(i18n, "key", name = expr)`
/// - `t!(i18n, "key", count = expr)` — triggers plural
#[proc_macro]
pub fn t(input: TokenStream) -> TokenStream {
    match t_impl(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn t_impl(_input: proc_macro2::TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    todo!()
}
