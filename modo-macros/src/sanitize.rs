use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Data, DeriveInput, Fields, Ident, LitInt, LitStr, Path, Result, Token, Type, parse2};

struct SanitizeField {
    field_name: Ident,
    field_type: Type,
    rules: Vec<SanitizationRule>,
}

enum SanitizationRule {
    Trim,
    Lowercase,
    Uppercase,
    StripHtml,
    CollapseWhitespace,
    Truncate { max_chars: LitInt },
    NormalizeEmail,
    Custom { func_path: Path },
}

/// Parse the content inside `#[clean(...)]`.
struct SanitizeAttr {
    rules: Vec<SanitizationRule>,
}

impl Parse for SanitizeAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut rules = Vec::new();

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            let name = ident.to_string();

            match name.as_str() {
                "trim" => rules.push(SanitizationRule::Trim),
                "lowercase" => rules.push(SanitizationRule::Lowercase),
                "uppercase" => rules.push(SanitizationRule::Uppercase),
                "strip_html_tags" => rules.push(SanitizationRule::StripHtml),
                "collapse_whitespace" => rules.push(SanitizationRule::CollapseWhitespace),
                "truncate" => {
                    input.parse::<Token![=]>()?;
                    let max_chars: LitInt = input.parse()?;
                    rules.push(SanitizationRule::Truncate { max_chars });
                }
                "normalize_email" => rules.push(SanitizationRule::NormalizeEmail),
                "custom" => {
                    input.parse::<Token![=]>()?;
                    let func_str: LitStr = input.parse()?;
                    let func_path: Path = func_str.parse()?;
                    rules.push(SanitizationRule::Custom { func_path });
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        format!("unknown sanitization rule: `{name}`"),
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(SanitizeAttr { rules })
    }
}

/// Returns true if the type is `Option<...>`.
fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(tp) = ty {
        tp.path
            .segments
            .last()
            .is_some_and(|seg| seg.ident == "Option")
    } else {
        false
    }
}

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
                "Sanitize can only be derived for structs",
            ));
        }
    };

    // Parse #[clean(...)] attributes on each field
    let mut sanitize_fields = Vec::new();
    for field in fields {
        let field_name = field.ident.clone().unwrap();
        let field_type = field.ty.clone();
        let mut all_rules = Vec::new();

        for attr in &field.attrs {
            if attr.path().is_ident("clean") {
                let parsed: SanitizeAttr = attr.parse_args()?;
                all_rules.extend(parsed.rules);
            }
        }

        if !all_rules.is_empty() {
            sanitize_fields.push(SanitizeField {
                field_name,
                field_type,
                rules: all_rules,
            });
        }
    }

    let field_sanitizations: Vec<TokenStream> =
        sanitize_fields.iter().map(gen_field_sanitization).collect();

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Generate a unique function name for the sanitizer trampoline
    let sanitize_fn_name = quote::format_ident!("__sanitize_{}", struct_name);

    Ok(quote! {
        impl #impl_generics modo::sanitize::Sanitize for #struct_name #ty_generics #where_clause {
            fn sanitize(&mut self) {
                #(#field_sanitizations)*
            }
        }

        // Auto-register for inventory-based lookup (used by extractors)
        #[allow(non_snake_case)]
        fn #sanitize_fn_name(any: &mut dyn std::any::Any) {
            if let Some(v) = any.downcast_mut::<#struct_name>() {
                <#struct_name as modo::sanitize::Sanitize>::sanitize(v);
            }
        }

        modo::inventory::submit!(modo::sanitize::SanitizerRegistration {
            type_id: std::any::TypeId::of::<#struct_name>(),
            sanitize: #sanitize_fn_name,
        });
    })
}

fn gen_field_sanitization(sf: &SanitizeField) -> TokenStream {
    let field_name = &sf.field_name;
    let is_option = is_option_type(&sf.field_type);

    let rule_calls: Vec<TokenStream> = sf
        .rules
        .iter()
        .map(|rule| match rule {
            SanitizationRule::Trim => quote! { __val = modo::sanitize::trim(__val); },
            SanitizationRule::Lowercase => quote! { __val = modo::sanitize::lowercase(__val); },
            SanitizationRule::Uppercase => quote! { __val = modo::sanitize::uppercase(__val); },
            SanitizationRule::StripHtml => {
                quote! { __val = modo::sanitize::strip_html_tags(__val); }
            }
            SanitizationRule::CollapseWhitespace => {
                quote! { __val = modo::sanitize::collapse_whitespace(__val); }
            }
            SanitizationRule::Truncate { max_chars } => {
                quote! { __val = modo::sanitize::truncate(__val, #max_chars); }
            }
            SanitizationRule::NormalizeEmail => {
                quote! { __val = modo::sanitize::normalize_email(__val); }
            }
            SanitizationRule::Custom { func_path } => {
                quote! { __val = #func_path(__val); }
            }
        })
        .collect();

    if is_option {
        quote! {
            if let Some(mut __val) = self.#field_name.take() {
                #(#rule_calls)*
                self.#field_name = Some(__val);
            }
        }
    } else {
        quote! {
            {
                let mut __val = std::mem::take(&mut self.#field_name);
                #(#rule_calls)*
                self.#field_name = __val;
            }
        }
    }
}
