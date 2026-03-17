use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Data, DeriveInput, Fields, Ident, LitInt, LitStr, Result, Token, Type, parse2};

/// Recognized field types for multipart parsing.
enum FieldKind {
    UploadedFile,
    OptionUploadedFile,
    VecUploadedFile,
    BufferedUpload,
    String,
    OptionString,
    /// Other type implementing FromStr.
    FromStr,
}

struct MultipartField {
    field_name: Ident,
    multipart_name: String,
    kind: FieldKind,
    upload_attrs: UploadAttrs,
}

#[derive(Default)]
struct UploadAttrs {
    max_size: Option<usize>,
    accept: Option<String>,
    min_count: Option<usize>,
    max_count: Option<usize>,
}

/// Parse the content inside `#[upload(...)]`.
struct UploadAttr {
    attrs: UploadAttrs,
}

impl Parse for UploadAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attrs = UploadAttrs::default();

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            let name = ident.to_string();

            match name.as_str() {
                "max_size" => {
                    input.parse::<Token![=]>()?;
                    let lit: LitStr = input.parse()?;
                    attrs.max_size = Some(
                        parse_size_str(&lit.value())
                            .map_err(|e| syn::Error::new_spanned(&lit, e))?,
                    );
                }
                "accept" => {
                    input.parse::<Token![=]>()?;
                    let lit: LitStr = input.parse()?;
                    attrs.accept = Some(lit.value());
                }
                "min_count" => {
                    input.parse::<Token![=]>()?;
                    let lit: LitInt = input.parse()?;
                    attrs.min_count = Some(lit.base10_parse()?);
                }
                "max_count" => {
                    input.parse::<Token![=]>()?;
                    let lit: LitInt = input.parse()?;
                    attrs.max_count = Some(lit.base10_parse()?);
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        format!("unknown upload attribute: `{name}`"),
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(UploadAttr { attrs })
    }
}

/// Extract the `rename = "..."` value from a `#[serde(...)]` attribute, if present.
fn parse_serde_rename(attr: &syn::Attribute) -> Result<Option<String>> {
    let mut rename = None;
    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("rename") {
            let value = meta.value()?;
            let lit: LitStr = value.parse()?;
            rename = Some(lit.value());
        }
        Ok(())
    })?;
    Ok(rename)
}

/// Parse a size string like "5mb", "100kb" into bytes at compile time.
fn parse_size_str(s: &str) -> std::result::Result<usize, String> {
    let s = s.trim().to_lowercase();
    let (num_str, multiplier) = if let Some(n) = s.strip_suffix("gb") {
        (n, 1024 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("mb") {
        (n, 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("kb") {
        (n, 1024)
    } else if let Some(n) = s.strip_suffix('b') {
        (n, 1)
    } else {
        (s.as_str(), 1)
    };

    let num: usize = num_str
        .trim()
        .parse()
        .map_err(|_| format!("invalid size number: `{num_str}`"))?;
    Ok(num * multiplier)
}

fn classify_type(ty: &Type) -> FieldKind {
    let Type::Path(type_path) = ty else {
        return FieldKind::FromStr;
    };
    let Some(last) = type_path.path.segments.last() else {
        return FieldKind::FromStr;
    };

    match last.ident.to_string().as_str() {
        "UploadedFile" => FieldKind::UploadedFile,
        "BufferedUpload" => FieldKind::BufferedUpload,
        "String" => FieldKind::String,
        "Option" => classify_inner_type(last, |name| match name {
            "UploadedFile" => Some(FieldKind::OptionUploadedFile),
            "String" => Some(FieldKind::OptionString),
            _ => None,
        }),
        "Vec" => classify_inner_type(last, |name| match name {
            "UploadedFile" => Some(FieldKind::VecUploadedFile),
            _ => None,
        }),
        _ => FieldKind::FromStr,
    }
}

fn classify_inner_type(
    seg: &syn::PathSegment,
    matcher: impl FnOnce(&str) -> Option<FieldKind>,
) -> FieldKind {
    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments
        && let Some(syn::GenericArgument::Type(Type::Path(tp))) = args.args.first()
        && let Some(inner_seg) = tp.path.segments.last()
        && let Some(kind) = matcher(&inner_seg.ident.to_string())
    {
        return kind;
    }
    FieldKind::FromStr
}

/// Expand the `FromMultipart` derive macro.
///
/// Generates a `FromMultipart` impl that:
/// - Iterates multipart fields and populates per-field variables.
/// - Applies the global `max_file_size` limit during streaming for all file fields.
/// - Performs post-collection validation (required checks, per-field `max_size` and
///   `accept` for `UploadedFile`/`Option<UploadedFile>`/`Vec<UploadedFile>`, and count
///   constraints for `Vec<UploadedFile>`).
/// - Constructs and returns `Self`.
pub fn expand(input: TokenStream) -> Result<TokenStream> {
    let input: DeriveInput = parse2(input)?;
    let struct_name = &input.ident;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    &input,
                    "only named fields are supported",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &input,
                "FromMultipart can only be derived for structs",
            ));
        }
    };

    let mut multipart_fields = Vec::new();
    let mut has_stream = false;

    for field in fields {
        let field_name = field.ident.clone().unwrap();
        let kind = classify_type(&field.ty);

        if matches!(kind, FieldKind::BufferedUpload) {
            has_stream = true;
        }

        // Parse #[serde(rename = "...")] for rename
        let mut multipart_name = field_name.to_string();
        let mut upload_attrs = UploadAttrs::default();

        for attr in &field.attrs {
            if attr.path().is_ident("serde")
                && let Some(name) = parse_serde_rename(attr)?
            {
                multipart_name = name;
            }
            if attr.path().is_ident("upload") {
                let parsed: UploadAttr = attr.parse_args()?;
                upload_attrs = parsed.attrs;
            }
        }

        multipart_fields.push(MultipartField {
            field_name,
            multipart_name,
            kind,
            upload_attrs,
        });
    }

    // Stream fields must be last (they consume the multipart iterator)
    if has_stream {
        let stream_count = multipart_fields
            .iter()
            .filter(|f| matches!(f.kind, FieldKind::BufferedUpload))
            .count();
        if stream_count > 1 {
            return Err(syn::Error::new_spanned(
                &input,
                "only one BufferedUpload field is allowed",
            ));
        }
    }

    // Generate variable declarations
    let var_decls: Vec<TokenStream> = multipart_fields
        .iter()
        .map(|f| {
            let var = quote::format_ident!("__{}", f.field_name);
            match &f.kind {
                FieldKind::UploadedFile => {
                    quote! { let mut #var: Option<modo_upload::UploadedFile> = None; }
                }
                FieldKind::OptionUploadedFile => {
                    quote! { let mut #var: Option<modo_upload::UploadedFile> = None; }
                }
                FieldKind::VecUploadedFile => {
                    quote! { let mut #var: Vec<modo_upload::UploadedFile> = Vec::new(); }
                }
                FieldKind::BufferedUpload => {
                    quote! { let mut #var: Option<modo_upload::BufferedUpload> = None; }
                }
                FieldKind::String => quote! { let mut #var: Option<String> = None; },
                FieldKind::OptionString => quote! { let mut #var: Option<String> = None; },
                FieldKind::FromStr => quote! { let mut #var: Option<String> = None; },
            }
        })
        .collect();

    // Generate match arms for field processing.
    // All file fields pass the global __max_file_size limit to from_field for streaming
    // enforcement. Per-field max_size and accept constraints are applied post-collection
    // for UploadedFile, Option<UploadedFile>, and Vec<UploadedFile> fields only.
    let match_arms: Vec<TokenStream> = multipart_fields
        .iter()
        .map(|f| {
            let var = quote::format_ident!("__{}", f.field_name);
            let name = &f.multipart_name;
            let file_size_limit = quote! { __max_file_size };
            match &f.kind {
                FieldKind::UploadedFile | FieldKind::OptionUploadedFile => quote! {
                    Some(#name) => {
                        #var = Some(modo_upload::UploadedFile::from_field(__field, #file_size_limit).await?);
                    }
                },
                FieldKind::VecUploadedFile => quote! {
                    Some(#name) => {
                        #var.push(modo_upload::UploadedFile::from_field(__field, #file_size_limit).await?);
                    }
                },
                FieldKind::BufferedUpload => quote! {
                    Some(#name) => {
                        #var = Some(modo_upload::BufferedUpload::from_field(__field, #file_size_limit).await?);
                    }
                },
                FieldKind::String | FieldKind::OptionString | FieldKind::FromStr => quote! {
                    Some(#name) => {
                        #var = Some(__field.text().await.map_err(|e| {
                            modo::HttpError::BadRequest.with_message(format!("{e}"))
                        })?);
                    }
                },
            }
        })
        .collect();

    let mut validation_stmts = Vec::new();
    let mut field_assignments = Vec::new();

    for f in &multipart_fields {
        let field_name = &f.field_name;
        let var = quote::format_ident!("__{}", f.field_name);
        let name_str = &f.multipart_name;

        match &f.kind {
            FieldKind::UploadedFile => {
                validation_stmts.push(quote! {
                    let #var = #var.ok_or_else(|| {
                        modo::validate::validation_error(vec![(#name_str, vec!["is required".into()])])
                    })?;
                });
                if let Some(max) = f.upload_attrs.max_size {
                    let msg = format!(
                        "File exceeds maximum size of {}",
                        format_size_for_codegen(max)
                    );
                    validation_stmts.push(quote! {
                        if #var.size() > #max {
                            return Err(modo::validate::validation_error(vec![(#name_str, vec![#msg.into()])]));
                        }
                    });
                }
                if let Some(ref accept) = f.upload_attrs.accept {
                    let msg = format!("File type must match {accept}");
                    validation_stmts.push(quote! {
                        if !modo_upload::__internal::mime_matches(#var.content_type(), #accept) {
                            return Err(modo::validate::validation_error(vec![(#name_str, vec![#msg.into()])]));
                        }
                    });
                }
                field_assignments.push(quote! { #field_name: #var });
            }
            FieldKind::OptionUploadedFile => {
                if let Some(max) = f.upload_attrs.max_size {
                    let msg = format!(
                        "File exceeds maximum size of {}",
                        format_size_for_codegen(max)
                    );
                    validation_stmts.push(quote! {
                        if let Some(ref __f) = #var {
                            if __f.size() > #max {
                                return Err(modo::validate::validation_error(vec![(#name_str, vec![#msg.into()])]));
                            }
                        }
                    });
                }
                if let Some(ref accept) = f.upload_attrs.accept {
                    let msg = format!("File type must match {accept}");
                    validation_stmts.push(quote! {
                        if let Some(ref __f) = #var {
                            if !modo_upload::__internal::mime_matches(__f.content_type(), #accept) {
                                return Err(modo::validate::validation_error(vec![(#name_str, vec![#msg.into()])]));
                            }
                        }
                    });
                }
                field_assignments.push(quote! { #field_name: #var });
            }
            FieldKind::VecUploadedFile => {
                if let Some(min) = f.upload_attrs.min_count {
                    let msg = format!("At least {min} file(s) required");
                    validation_stmts.push(quote! {
                        if #var.len() < #min {
                            return Err(modo::validate::validation_error(vec![(#name_str, vec![#msg.into()])]));
                        }
                    });
                }
                if let Some(max) = f.upload_attrs.max_count {
                    let msg = format!("At most {max} file(s) allowed");
                    validation_stmts.push(quote! {
                        if #var.len() > #max {
                            return Err(modo::validate::validation_error(vec![(#name_str, vec![#msg.into()])]));
                        }
                    });
                }
                if let Some(max_size) = f.upload_attrs.max_size {
                    let msg = format!(
                        "File exceeds maximum size of {}",
                        format_size_for_codegen(max_size)
                    );
                    validation_stmts.push(quote! {
                        for __f in &#var {
                            if __f.size() > #max_size {
                                return Err(modo::validate::validation_error(vec![(#name_str, vec![#msg.into()])]));
                            }
                        }
                    });
                }
                if let Some(ref accept) = f.upload_attrs.accept {
                    let msg = format!("File type must match {accept}");
                    validation_stmts.push(quote! {
                        for __f in &#var {
                            if !modo_upload::__internal::mime_matches(__f.content_type(), #accept) {
                                return Err(modo::validate::validation_error(vec![(#name_str, vec![#msg.into()])]));
                            }
                        }
                    });
                }
                field_assignments.push(quote! { #field_name: #var });
            }
            FieldKind::BufferedUpload => {
                validation_stmts.push(quote! {
                    let #var = #var.ok_or_else(|| {
                        modo::validate::validation_error(vec![(#name_str, vec!["is required".into()])])
                    })?;
                });
                field_assignments.push(quote! { #field_name: #var });
            }
            FieldKind::String => {
                validation_stmts.push(quote! {
                    let #var = #var.ok_or_else(|| {
                        modo::validate::validation_error(vec![(#name_str, vec!["is required".into()])])
                    })?;
                });
                field_assignments.push(quote! { #field_name: #var });
            }
            FieldKind::OptionString => {
                field_assignments.push(quote! { #field_name: #var });
            }
            FieldKind::FromStr => {
                let var2 = quote::format_ident!("__{}_parsed", f.field_name);
                validation_stmts.push(quote! {
                    let #var = #var.ok_or_else(|| {
                        modo::validate::validation_error(vec![(#name_str, vec!["is required".into()])])
                    })?;
                    let #var2 = #var.parse().map_err(|_| {
                        modo::validate::validation_error(vec![(#name_str, vec!["invalid value".into()])])
                    })?;
                });
                field_assignments.push(quote! { #field_name: #var2 });
            }
        }
    }

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics modo_upload::FromMultipart for #struct_name #ty_generics #where_clause {
            fn from_multipart(
                multipart: &mut modo_upload::__internal::axum::extract::Multipart,
                __max_file_size: Option<usize>,
            ) -> impl std::future::Future<Output = Result<Self, modo::Error>> + Send {
                async move {
                    #(#var_decls)*

                    while let Some(__field) = multipart.next_field().await
                        .map_err(|e| modo::HttpError::BadRequest.with_message(format!("{e}")))?
                    {
                        match __field.name() {
                            #(#match_arms)*
                            _ => {}
                        }
                    }

                    #(#validation_stmts)*

                    Ok(Self {
                        #(#field_assignments),*
                    })
                }
            }
        }
    })
}

fn format_size_for_codegen(bytes: usize) -> String {
    if bytes >= 1024 * 1024 * 1024 && bytes.is_multiple_of(1024 * 1024 * 1024) {
        format!("{}GB", bytes / (1024 * 1024 * 1024))
    } else if bytes >= 1024 * 1024 && bytes.is_multiple_of(1024 * 1024) {
        format!("{}MB", bytes / (1024 * 1024))
    } else if bytes >= 1024 && bytes.is_multiple_of(1024) {
        format!("{}KB", bytes / 1024)
    } else {
        format!("{bytes}B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_size_str --

    #[test]
    fn parse_size_bytes() {
        assert_eq!(parse_size_str("100b").unwrap(), 100);
    }

    #[test]
    fn parse_size_kilobytes() {
        assert_eq!(parse_size_str("5kb").unwrap(), 5 * 1024);
    }

    #[test]
    fn parse_size_megabytes() {
        assert_eq!(parse_size_str("10mb").unwrap(), 10 * 1024 * 1024);
    }

    #[test]
    fn parse_size_gigabytes() {
        assert_eq!(parse_size_str("2gb").unwrap(), 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_size_case_insensitive() {
        assert_eq!(parse_size_str("5MB").unwrap(), 5 * 1024 * 1024);
    }

    #[test]
    fn parse_size_whitespace() {
        assert_eq!(parse_size_str("  10mb  ").unwrap(), 10 * 1024 * 1024);
    }

    #[test]
    fn parse_size_plain_number() {
        assert_eq!(parse_size_str("1024").unwrap(), 1024);
    }

    #[test]
    fn parse_size_invalid() {
        assert!(parse_size_str("abcmb").is_err());
    }

    #[test]
    fn parse_size_zero() {
        assert_eq!(parse_size_str("0mb").unwrap(), 0);
    }

    // -- format_size_for_codegen --

    #[test]
    fn format_codegen_bytes() {
        assert_eq!(format_size_for_codegen(500), "500B");
    }

    #[test]
    fn format_codegen_kilobytes() {
        assert_eq!(format_size_for_codegen(1024), "1KB");
    }

    #[test]
    fn format_codegen_megabytes() {
        assert_eq!(format_size_for_codegen(5 * 1024 * 1024), "5MB");
    }

    #[test]
    fn format_codegen_gigabytes() {
        assert_eq!(format_size_for_codegen(2 * 1024 * 1024 * 1024), "2GB");
    }

    #[test]
    fn format_codegen_non_aligned() {
        assert_eq!(format_size_for_codegen(1025), "1025B");
    }

    #[test]
    fn parse_size_negative() {
        assert!(parse_size_str("-5mb").is_err());
    }

    #[test]
    fn parse_size_fractional() {
        assert!(parse_size_str("1.5mb").is_err());
    }

    #[test]
    fn parse_size_space_between_number_and_unit() {
        assert_eq!(parse_size_str("5 mb").unwrap(), 5 * 1024 * 1024);
    }

    #[test]
    fn parse_size_empty_string() {
        assert!(parse_size_str("").is_err());
    }

    #[test]
    fn format_codegen_zero() {
        assert_eq!(format_size_for_codegen(0), "0B");
    }
}
