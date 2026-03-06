use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, LitInt, LitStr, Result, Token, parse2};

struct MigrationArgs {
    version: u64,
    description: String,
}

impl syn::parse::Parse for MigrationArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut version = None;
        let mut description = None;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match ident.to_string().as_str() {
                "version" => {
                    let val: LitInt = input.parse()?;
                    version = Some(val.base10_parse::<u64>()?);
                }
                "description" => {
                    let val: LitStr = input.parse()?;
                    description = Some(val.value());
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        format!("unknown migration attribute: {other}"),
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let version = version.ok_or_else(|| input.error("missing `version` argument"))?;
        let description =
            description.ok_or_else(|| input.error("missing `description` argument"))?;

        Ok(MigrationArgs {
            version,
            description,
        })
    }
}

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args: MigrationArgs = parse2(attr)?;
    let func: ItemFn = parse2(item)?;

    let func_name = &func.sig.ident;
    let version = args.version;
    let description = &args.description;

    Ok(quote! {
        #func

        modo_db::inventory::submit! {
            modo_db::MigrationRegistration {
                version: #version,
                description: #description,
                handler: |db| Box::pin(#func_name(db)),
            }
        }
    })
}
