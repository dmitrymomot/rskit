use proc_macro2::TokenStream;
use syn::Result;

/// Returns true if the last path segment of `ty` matches `name`.
/// Replaces per-file `is_option_type()` and `is_string_type()` helpers.
pub(crate) fn is_type_named(ty: &syn::Type, name: &str) -> bool {
    if let syn::Type::Path(tp) = ty {
        tp.path.segments.last().is_some_and(|seg| seg.ident == name)
    } else {
        false
    }
}

/// Parse an optional `name = "..."` attribute, returning `default` when the attr is empty.
/// Shared by `#[template_filter]` and `#[template_function]`.
pub(crate) fn parse_name_attr(attr: TokenStream, default: String) -> Result<String> {
    if attr.is_empty() {
        return Ok(default);
    }
    let name_value: syn::MetaNameValue = syn::parse2(attr)?;
    if !name_value.path.is_ident("name") {
        return Err(syn::Error::new_spanned(
            &name_value.path,
            "unknown attribute, expected `name`",
        ));
    }
    if let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(s),
        ..
    }) = &name_value.value
    {
        Ok(s.value())
    } else {
        Err(syn::Error::new_spanned(
            &name_value.value,
            "expected string literal for `name`",
        ))
    }
}
