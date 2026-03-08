use proc_macro::TokenStream;

/// Marks a struct as a view with an associated template.
///
/// Usage:
/// ```ignore
/// #[view("pages/home.html")]
/// struct HomePage { items: Vec<Item> }
///
/// #[view("pages/login.html", htmx = "htmx/login_form.html")]
/// struct LoginPage { form_errors: Vec<String> }
/// ```
#[proc_macro_attribute]
pub fn view(attr: TokenStream, item: TokenStream) -> TokenStream {
    match view_impl(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn view_impl(
    _attr: proc_macro2::TokenStream,
    _item: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    todo!("implement in Task 4")
}
