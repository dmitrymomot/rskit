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

    // Find #[base], #[user], and #[session] fields
    let mut base_field = None;
    let mut user_field = None;
    let mut user_inner_type = None;
    let mut session_field = None;
    let mut clean_fields = Vec::new();

    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap();
        let field_ty = &field.ty;
        let field_vis = &field.vis;

        let has_base = field.attrs.iter().any(|a| a.path().is_ident("base"));
        let has_user = field.attrs.iter().any(|a| a.path().is_ident("user"));
        let has_session = field.attrs.iter().any(|a| a.path().is_ident("session"));

        if has_base {
            if base_field.is_some() {
                return Err(syn::Error::new_spanned(
                    field_name,
                    "only one #[base] field is allowed",
                ));
            }
            base_field = Some(field_name.clone());
        }

        if has_user {
            if user_field.is_some() {
                return Err(syn::Error::new_spanned(
                    field_name,
                    "only one #[user] field is allowed",
                ));
            }
            user_field = Some(field_name.clone());
            user_inner_type = extract_option_inner(field_ty);
            if user_inner_type.is_none() {
                return Err(syn::Error::new_spanned(
                    field_ty,
                    "#[user] field must be Option<T>",
                ));
            }
        }

        if has_session {
            if session_field.is_some() {
                return Err(syn::Error::new_spanned(
                    field_name,
                    "only one #[session] field is allowed",
                ));
            }
            if extract_option_inner(field_ty).is_none() {
                return Err(syn::Error::new_spanned(
                    field_ty,
                    "#[session] field must be Option<SessionData>",
                ));
            }
            session_field = Some(field_name.clone());
        }

        if !has_base && !has_user && !has_session {
            return Err(syn::Error::new_spanned(
                field_name,
                "#[rskit::context] structs may only contain #[base], #[user], and #[session] fields",
            ));
        }

        // Strip #[base], #[user], and #[session] attributes from output
        let clean_attrs: Vec<_> = field
            .attrs
            .iter()
            .filter(|a| {
                !a.path().is_ident("base")
                    && !a.path().is_ident("user")
                    && !a.path().is_ident("session")
            })
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

    // Generate user extraction code
    let user_extraction = if let (Some(user_name), Some(inner_ty)) = (&user_field, &user_inner_type)
    {
        quote! {
            let #user_name = match rskit::extractors::auth::OptionalAuth::<#inner_ty>::from_request_parts(parts, state).await {
                Ok(rskit::extractors::auth::OptionalAuth(Some(auth_data))) => Some(auth_data.user),
                Ok(rskit::extractors::auth::OptionalAuth(None)) => None,
                Err(never) => match never {},
            };
        }
    } else {
        quote! {}
    };

    // Generate session extraction code
    let session_extraction = if let Some(session_name) = &session_field {
        quote! {
            let #session_name = parts.extensions.get::<rskit::session::SessionData>().cloned();
        }
    } else {
        quote! {}
    };

    // Build field list for struct construction
    let mut construct_fields = vec![quote! { #base_name }];
    if let Some(ref user_name) = user_field {
        construct_fields.push(quote! { #user_name });
    }
    if let Some(ref session_name) = session_field {
        construct_fields.push(quote! { #session_name });
    }

    let from_request_impl = quote! {
        impl rskit::axum::extract::FromRequestParts<rskit::app::AppState> for #struct_name {
            type Rejection = std::convert::Infallible;

            async fn from_request_parts(
                parts: &mut rskit::axum::http::request::Parts,
                state: &rskit::app::AppState,
            ) -> std::result::Result<Self, Self::Rejection> {
                let #base_name = rskit::templates::BaseContext::from_request_parts(parts, state).await?;
                #user_extraction
                #session_extraction
                Ok(Self { #(#construct_fields),* })
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
