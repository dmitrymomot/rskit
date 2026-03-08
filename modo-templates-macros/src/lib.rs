use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, ItemStruct, LitStr, Token};

struct ViewAttr {
    template: LitStr,
    htmx_template: Option<LitStr>,
}

impl Parse for ViewAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let template: LitStr = input.parse()?;
        let mut htmx_template = None;

        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            if key == "htmx" {
                input.parse::<Token![=]>()?;
                htmx_template = Some(input.parse::<LitStr>()?);
            } else {
                return Err(syn::Error::new_spanned(
                    key,
                    "unknown attribute, expected `htmx`",
                ));
            }
        }

        Ok(ViewAttr {
            template,
            htmx_template,
        })
    }
}

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
    attr: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let attr = syn::parse2::<ViewAttr>(attr)?;
    let input = syn::parse2::<ItemStruct>(item)?;

    let struct_name = &input.ident;
    let template_path = &attr.template;

    let view_construction = match &attr.htmx_template {
        Some(htmx_lit) => quote! {
            ::modo_templates::View::new(#template_path, user_context)
                .with_htmx(#htmx_lit)
        },
        None => quote! {
            ::modo_templates::View::new(#template_path, user_context)
        },
    };

    Ok(quote! {
        #[derive(::modo_templates::serde::Serialize)]
        #[serde(crate = "::modo_templates::serde")]
        #input

        impl ::modo_templates::axum::response::IntoResponse for #struct_name {
            fn into_response(self) -> ::modo_templates::axum::response::Response {
                let user_context = ::modo_templates::minijinja::Value::from_serialize(&self);
                let view = #view_construction;
                view.into_response()
            }
        }
    })
}
