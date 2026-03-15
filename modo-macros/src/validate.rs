use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{
    Data, DeriveInput, Fields, Ident, Lit, LitInt, LitStr, Path, Result, Token, Type, parse2,
};

struct ValidateField {
    field_name: Ident,
    field_type: Type,
    rules: Vec<ValidationRule>,
    field_message: Option<String>,
}

enum ValidationRule {
    Required {
        message: Option<String>,
    },
    MinLength {
        value: LitInt,
        message: Option<String>,
    },
    MaxLength {
        value: LitInt,
        message: Option<String>,
    },
    Email {
        message: Option<String>,
    },
    Min {
        value: Lit,
        message: Option<String>,
    },
    Max {
        value: Lit,
        message: Option<String>,
    },
    Custom {
        func_path: Path,
        message: Option<String>,
    },
}

/// Parse the content inside `#[validate(...)]`.
struct ValidateAttr {
    rules: Vec<ValidationRule>,
    field_message: Option<String>,
}

impl Parse for ValidateAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut rules = Vec::new();
        let mut field_message = None;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            let name = ident.to_string();

            match name.as_str() {
                "required" => {
                    let message = parse_rule_message(input)?;
                    rules.push(ValidationRule::Required { message });
                }
                "min_length" => {
                    input.parse::<Token![=]>()?;
                    let value: LitInt = input.parse()?;
                    let message = parse_rule_message(input)?;
                    rules.push(ValidationRule::MinLength { value, message });
                }
                "max_length" => {
                    input.parse::<Token![=]>()?;
                    let value: LitInt = input.parse()?;
                    let message = parse_rule_message(input)?;
                    rules.push(ValidationRule::MaxLength { value, message });
                }
                "email" => {
                    let message = parse_rule_message(input)?;
                    rules.push(ValidationRule::Email { message });
                }
                "min" => {
                    input.parse::<Token![=]>()?;
                    let value: Lit = input.parse()?;
                    let message = parse_rule_message(input)?;
                    rules.push(ValidationRule::Min { value, message });
                }
                "max" => {
                    input.parse::<Token![=]>()?;
                    let value: Lit = input.parse()?;
                    let message = parse_rule_message(input)?;
                    rules.push(ValidationRule::Max { value, message });
                }
                "custom" => {
                    input.parse::<Token![=]>()?;
                    let func_str: LitStr = input.parse()?;
                    let func_path: Path = func_str.parse()?;
                    let message = parse_rule_message(input)?;
                    rules.push(ValidationRule::Custom { func_path, message });
                }
                "message" => {
                    input.parse::<Token![=]>()?;
                    let msg: LitStr = input.parse()?;
                    field_message = Some(msg.value());
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        format!("unknown validation rule: `{name}`"),
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(ValidateAttr {
            rules,
            field_message,
        })
    }
}

/// Try to parse an optional `(message = "...")` after a rule name/value.
fn parse_rule_message(input: ParseStream) -> Result<Option<String>> {
    if input.peek(syn::token::Paren) {
        let content;
        syn::parenthesized!(content in input);
        let ident: Ident = content.parse()?;
        if ident != "message" {
            return Err(syn::Error::new_spanned(ident, "expected `message`"));
        }
        content.parse::<Token![=]>()?;
        let msg: LitStr = content.parse()?;
        Ok(Some(msg.value()))
    } else {
        Ok(None)
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
                "Validate can only be derived for structs",
            ));
        }
    };

    // Parse #[validate(...)] attributes on each field
    let mut validate_fields = Vec::new();
    for field in fields {
        let field_name = field.ident.clone().unwrap();
        let field_type = field.ty.clone();
        let mut all_rules = Vec::new();
        let mut field_message = None;

        for attr in &field.attrs {
            if attr.path().is_ident("validate") {
                let parsed: ValidateAttr = attr.parse_args()?;
                all_rules.extend(parsed.rules);
                if parsed.field_message.is_some() {
                    field_message = parsed.field_message;
                }
            }
        }

        if !all_rules.is_empty() || field_message.is_some() {
            validate_fields.push(ValidateField {
                field_name,
                field_type,
                rules: all_rules,
                field_message,
            });
        }
    }

    // Generate validation code for each field
    let field_checks: Vec<TokenStream> = validate_fields.iter().map(gen_field_validation).collect();

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Defer Vec allocation to error path
    let body = if validate_fields.is_empty() {
        quote! { Ok(()) }
    } else {
        let error_idents: Vec<proc_macro2::Ident> = validate_fields
            .iter()
            .map(|vf| quote::format_ident!("__errors_{}", vf.field_name))
            .collect();
        let empty_checks: Vec<TokenStream> = error_idents
            .iter()
            .map(|id| quote! { #id.is_empty() })
            .collect();
        let field_collect: Vec<TokenStream> = validate_fields
            .iter()
            .zip(error_idents.iter())
            .map(|(vf, id)| {
                let name_str = vf.field_name.to_string();
                quote! { (#name_str, #id) }
            })
            .collect();
        quote! {
            if #(#empty_checks)&&* {
                Ok(())
            } else {
                Err(modo::validate::validation_error(vec![#(#field_collect),*]))
            }
        }
    };

    Ok(quote! {
        impl #impl_generics modo::validate::Validate for #struct_name #ty_generics #where_clause {
            fn validate(&self) -> Result<(), modo::Error> {
                #(#field_checks)*
                #body
            }
        }
    })
}

struct FieldContext<'a> {
    field_name: &'a Ident,
    is_option: bool,
    is_string: bool,
    has_required: bool,
    has_field_msg: bool,
    field_message: &'a Option<String>,
}

fn gen_field_validation(vf: &ValidateField) -> TokenStream {
    let field_name = &vf.field_name;
    let errors_ident = quote::format_ident!("__errors_{}", field_name);
    let has_field_msg = vf.field_message.is_some();

    let ctx = FieldContext {
        field_name,
        is_option: crate::utils::is_type_named(&vf.field_type, "Option"),
        is_string: crate::utils::is_type_named(&vf.field_type, "String"),
        has_required: vf
            .rules
            .iter()
            .any(|r| matches!(r, ValidationRule::Required { .. })),
        has_field_msg,
        field_message: &vf.field_message,
    };

    let field_msg_added_decl = if has_field_msg {
        quote! { let mut __field_msg_added = false; }
    } else {
        quote! {}
    };

    let rule_checks: Vec<TokenStream> = vf
        .rules
        .iter()
        .map(|rule| gen_rule_check(rule, &ctx))
        .collect();

    quote! {
        let mut #errors_ident: Vec<String> = Vec::new();
        #field_msg_added_decl
        #(#rule_checks)*
    }
}

fn gen_rule_check(rule: &ValidationRule, ctx: &FieldContext) -> TokenStream {
    let field_name = ctx.field_name;
    let is_option = ctx.is_option;
    let is_string = ctx.is_string;
    let has_required = ctx.has_required;
    let has_field_msg = ctx.has_field_msg;
    let field_message = ctx.field_message;
    let errors_ident = quote::format_ident!("__errors_{}", field_name);

    match rule {
        ValidationRule::Required { message } => {
            let push = gen_push(
                &errors_ident,
                message,
                has_field_msg,
                field_message,
                "is required",
            );
            if is_option {
                quote! {
                    if self.#field_name.is_none() {
                        #push
                    }
                }
            } else if is_string {
                quote! {
                    if self.#field_name.is_empty() {
                        #push
                    }
                }
            } else {
                // Non-option, non-string: always present after deserialization, no-op
                quote! {}
            }
        }
        ValidationRule::MinLength { value, message } => {
            let default_msg = format!("must be at least {} characters", value);
            let push = gen_push(
                &errors_ident,
                message,
                has_field_msg,
                field_message,
                &default_msg,
            );
            let check = quote! { __val.chars().count() < #value };
            wrap_non_required_check(
                field_name,
                &push,
                &check,
                is_option,
                is_string,
                has_required,
            )
        }
        ValidationRule::MaxLength { value, message } => {
            let default_msg = format!("must be at most {} characters", value);
            let push = gen_push(
                &errors_ident,
                message,
                has_field_msg,
                field_message,
                &default_msg,
            );
            let check = quote! { __val.chars().count() > #value };
            wrap_non_required_check(
                field_name,
                &push,
                &check,
                is_option,
                is_string,
                has_required,
            )
        }
        ValidationRule::Email { message } => {
            let push = gen_push(
                &errors_ident,
                message,
                has_field_msg,
                field_message,
                "must be a valid email address",
            );
            let check = quote! { !modo::validate::is_valid_email(__val) };
            wrap_non_required_check(
                field_name,
                &push,
                &check,
                is_option,
                is_string,
                has_required,
            )
        }
        ValidationRule::Min { value, message } => {
            let default_msg = format!("must be at least {}", lit_to_string(value));
            let push = gen_push(
                &errors_ident,
                message,
                has_field_msg,
                field_message,
                &default_msg,
            );
            if is_option {
                quote! {
                    if let Some(ref __val) = self.#field_name {
                        if *__val < #value {
                            #push
                        }
                    }
                }
            } else {
                quote! {
                    if self.#field_name < #value {
                        #push
                    }
                }
            }
        }
        ValidationRule::Max { value, message } => {
            let default_msg = format!("must be at most {}", lit_to_string(value));
            let push = gen_push(
                &errors_ident,
                message,
                has_field_msg,
                field_message,
                &default_msg,
            );
            if is_option {
                quote! {
                    if let Some(ref __val) = self.#field_name {
                        if *__val > #value {
                            #push
                        }
                    }
                }
            } else {
                quote! {
                    if self.#field_name > #value {
                        #push
                    }
                }
            }
        }
        ValidationRule::Custom { func_path, message } => {
            if is_option {
                if let Some(msg) = message {
                    quote! {
                        if let Some(ref __val) = self.#field_name {
                            if #func_path(__val).is_err() {
                                #errors_ident.push(#msg.to_owned());
                            }
                        }
                    }
                } else if has_field_msg {
                    let fm = field_message.as_ref().unwrap();
                    quote! {
                        if let Some(ref __val) = self.#field_name {
                            if #func_path(__val).is_err() {
                                if !__field_msg_added {
                                    #errors_ident.push(#fm.to_owned());
                                    __field_msg_added = true;
                                }
                            }
                        }
                    }
                } else {
                    quote! {
                        if let Some(ref __val) = self.#field_name {
                            if let Err(__e) = #func_path(__val) {
                                #errors_ident.push(__e);
                            }
                        }
                    }
                }
            } else if let Some(msg) = message {
                quote! {
                    if #func_path(&self.#field_name).is_err() {
                        #errors_ident.push(#msg.to_owned());
                    }
                }
            } else if has_field_msg {
                let fm = field_message.as_ref().unwrap();
                quote! {
                    if #func_path(&self.#field_name).is_err() {
                        if !__field_msg_added {
                            #errors_ident.push(#fm.to_owned());
                            __field_msg_added = true;
                        }
                    }
                }
            } else {
                quote! {
                    if let Err(__e) = #func_path(&self.#field_name) {
                        #errors_ident.push(__e);
                    }
                }
            }
        }
    }
}

/// Generate the push statement respecting message priority:
/// 1. Per-rule message (always used if present)
/// 2. Field-level message (used once, tracked by `__field_msg_added`)
/// 3. Default English message
fn gen_push(
    errors_ident: &proc_macro2::Ident,
    rule_message: &Option<String>,
    has_field_msg: bool,
    field_message: &Option<String>,
    default_msg: &str,
) -> TokenStream {
    if let Some(msg) = rule_message {
        quote! { #errors_ident.push(#msg.to_owned()); }
    } else if has_field_msg {
        let fm = field_message.as_ref().unwrap();
        quote! {
            if !__field_msg_added {
                #errors_ident.push(#fm.to_owned());
                __field_msg_added = true;
            }
        }
    } else {
        quote! { #errors_ident.push(#default_msg.to_owned()); }
    }
}

/// Wrap a non-required rule check with a guard so we don't double-report on empty/None values.
/// For Option types: `if let Some(ref __val) = self.field { if <check> { push } }`
/// For String with `required`: `if !self.field.is_empty() { if <check> { push } }`
/// For String without `required`: use `&self.field` as `__val` directly
/// For other types: direct check (non-applicable, but fallback)
fn wrap_non_required_check(
    field_name: &Ident,
    push: &TokenStream,
    check: &TokenStream,
    is_option: bool,
    is_string: bool,
    has_required: bool,
) -> TokenStream {
    if is_option {
        quote! {
            if let Some(ref __val) = self.#field_name {
                if #check {
                    #push
                }
            }
        }
    } else if is_string && has_required {
        quote! {
            if !self.#field_name.is_empty() {
                let __val = &self.#field_name;
                if #check {
                    #push
                }
            }
        }
    } else if is_string {
        quote! {
            {
                let __val = &self.#field_name;
                if #check {
                    #push
                }
            }
        }
    } else {
        // For non-string, non-option types, these rules (min_length, max_length, email)
        // don't typically apply, but generate the code anyway with __val = &self.field
        quote! {
            {
                let __val = &self.#field_name;
                if #check {
                    #push
                }
            }
        }
    }
}

fn lit_to_string(lit: &Lit) -> String {
    match lit {
        Lit::Int(i) => i.to_string(),
        Lit::Float(f) => f.to_string(),
        Lit::Str(s) => s.value(),
        _ => "?".to_string(),
    }
}
