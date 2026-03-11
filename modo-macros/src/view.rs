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

pub fn expand(
    attr: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let attr = syn::parse2::<ViewAttr>(attr)?;
    let input = syn::parse2::<ItemStruct>(item)?;

    let struct_name = &input.ident;
    let template_path = &attr.template;

    let view_construction = match &attr.htmx_template {
        Some(htmx_lit) => quote! {
            ::modo::templates::View::new(#template_path, user_context)
                .with_htmx(#htmx_lit)
        },
        None => quote! {
            ::modo::templates::View::new(#template_path, user_context)
        },
    };

    let htmx_template_expr = match &attr.htmx_template {
        Some(htmx_lit) => quote! { #htmx_lit },
        None => quote! { #template_path },
    };

    let has_dual = attr.htmx_template.is_some();

    Ok(quote! {
        #[derive(::modo::serde::Serialize)]
        #[serde(crate = "::modo::serde")]
        #input

        impl ::modo::axum::response::IntoResponse for #struct_name {
            fn into_response(self) -> ::modo::axum::response::Response {
                let user_context = ::modo::minijinja::Value::from_serialize(&self);
                let view = #view_construction;
                view.into_response()
            }
        }

        impl ::modo::templates::ViewRender for #struct_name {
            fn has_dual_template(&self) -> bool {
                #has_dual
            }

            fn render_with(
                &self,
                engine: &::modo::templates::TemplateEngine,
                context: &::modo::templates::TemplateContext,
                is_htmx: bool,
            ) -> Result<String, ::modo::templates::TemplateError> {
                let user_context = ::modo::minijinja::Value::from_serialize(&self);
                let template = if is_htmx {
                    #htmx_template_expr
                } else {
                    #template_path
                };
                let merged = context.merge_with(user_context);
                engine.render(template, merged)
            }
        }
    })
}
