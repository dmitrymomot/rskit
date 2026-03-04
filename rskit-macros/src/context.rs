use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, ItemStruct, Result, parse2};

pub fn expand(_attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let input: ItemStruct = parse2(item)?;
    let struct_name = &input.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;

    let fields = match &input.fields {
        Fields::Named(f) => &f.named,
        _ => {
            return Err(syn::Error::new_spanned(
                &input,
                "#[rskit::context] requires a struct with named fields",
            ));
        }
    };

    // Find #[base] and #[auth] fields
    let mut base_field = None;
    let mut auth_field = None;
    let mut auth_inner_type = None;
    let mut clean_fields = Vec::new();

    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap();
        let field_ty = &field.ty;
        let field_vis = &field.vis;

        let has_base = field.attrs.iter().any(|a| a.path().is_ident("base"));
        let has_auth = field.attrs.iter().any(|a| a.path().is_ident("auth"));

        if has_base {
            if base_field.is_some() {
                return Err(syn::Error::new_spanned(
                    field_name,
                    "only one #[base] field is allowed",
                ));
            }
            base_field = Some(field_name.clone());
        }

        if has_auth {
            if auth_field.is_some() {
                return Err(syn::Error::new_spanned(
                    field_name,
                    "only one #[auth] field is allowed",
                ));
            }
            auth_field = Some(field_name.clone());
            // Extract inner type from Option<T>
            auth_inner_type = extract_option_inner(field_ty);
            if auth_inner_type.is_none() {
                return Err(syn::Error::new_spanned(
                    field_ty,
                    "#[auth] field must be Option<T>",
                ));
            }
        }

        // Strip #[base] and #[auth] attributes from output
        let clean_attrs: Vec<_> = field
            .attrs
            .iter()
            .filter(|a| !a.path().is_ident("base") && !a.path().is_ident("auth"))
            .collect();

        clean_fields.push(quote! {
            #(#clean_attrs)*
            #field_vis #field_name: #field_ty
        });
    }

    let base_name = base_field.ok_or_else(|| {
        syn::Error::new_spanned(
            &input,
            "#[rskit::context] requires exactly one #[base] field",
        )
    })?;

    // Generate FromRequestParts impl
    let from_request_impl = if let (Some(auth_name), Some(inner_ty)) =
        (&auth_field, &auth_inner_type)
    {
        quote! {
            impl rskit::axum::extract::FromRequestParts<rskit::app::AppState> for #struct_name {
                type Rejection = std::convert::Infallible;

                async fn from_request_parts(
                    parts: &mut rskit::axum::http::request::Parts,
                    state: &rskit::app::AppState,
                ) -> std::result::Result<Self, Self::Rejection> {
                    let #base_name = rskit::templates::BaseContext::from_request_parts(parts, state).await?;

                    let #auth_name = parts
                        .extensions
                        .get::<rskit::session::SessionData>()
                        .and_then(|_session| {
                            parts.extensions.get::<#inner_ty>().cloned()
                        });

                    Ok(Self { #base_name, #auth_name })
                }
            }
        }
    } else {
        quote! {
            impl rskit::axum::extract::FromRequestParts<rskit::app::AppState> for #struct_name {
                type Rejection = std::convert::Infallible;

                async fn from_request_parts(
                    parts: &mut rskit::axum::http::request::Parts,
                    state: &rskit::app::AppState,
                ) -> std::result::Result<Self, Self::Rejection> {
                    let #base_name = rskit::templates::BaseContext::from_request_parts(parts, state).await?;
                    Ok(Self { #base_name })
                }
            }
        }
    };

    Ok(quote! {
        #(#attrs)*
        #vis struct #struct_name {
            #(#clean_fields),*
        }

        #from_request_impl
    })
}

/// Extract the inner type T from Option<T>.
fn extract_option_inner(ty: &syn::Type) -> Option<syn::Type> {
    if let syn::Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        if segment.ident == "Option"
            && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
            && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
        {
            return Some(inner.clone());
        }
    }
    None
}
