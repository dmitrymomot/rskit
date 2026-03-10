use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, LitStr, Token};

struct TInput {
    i18n_expr: Expr,
    key: LitStr,
    vars: Vec<(Ident, Expr)>,
}

impl Parse for TInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let i18n_expr: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let key: LitStr = input.parse()?;

        let mut vars = Vec::new();
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let name: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let value: Expr = input.parse()?;
            vars.push((name, value));
        }

        Ok(TInput {
            i18n_expr,
            key,
            vars,
        })
    }
}

pub fn expand(input: proc_macro2::TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    let input = syn::parse2::<TInput>(input)?;
    let i18n = &input.i18n_expr;
    let key = &input.key;

    let has_count = input.vars.iter().any(|(name, _)| name == "count");

    let var_pairs: Vec<proc_macro2::TokenStream> = input
        .vars
        .iter()
        .map(|(name, value)| {
            let name_str = name.to_string();
            quote! { (#name_str, &(#value).to_string()) }
        })
        .collect();

    if has_count {
        let count_expr = input
            .vars
            .iter()
            .find(|(name, _)| name == "count")
            .map(|(_, expr)| expr)
            .unwrap();

        Ok(quote! {
            #i18n.t_plural(#key, #count_expr as u64, &[#(#var_pairs),*])
        })
    } else {
        Ok(quote! {
            #i18n.t(#key, &[#(#var_pairs),*])
        })
    }
}
