use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, LitStr, Path, Result, Token};

pub struct RolesArgs {
    pub tenant_type: Path,
    pub member_type: Path,
    pub roles: Vec<LitStr>,
}

impl syn::parse::Parse for RolesArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let tenant_type: Path = input.parse()?;
        input.parse::<Token![,]>()?;
        let member_type: Path = input.parse()?;
        input.parse::<Token![,]>()?;
        let roles = input.parse_terminated(|input| input.parse::<LitStr>(), Token![,])?;
        Ok(RolesArgs {
            tenant_type,
            member_type,
            roles: roles.into_iter().collect(),
        })
    }
}

pub fn expand_allow_roles(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args: RolesArgs = syn::parse2(attr)?;
    let func: ItemFn = syn::parse2(item)?;
    let role_strs: Vec<&LitStr> = args.roles.iter().collect();
    let tenant_type = &args.tenant_type;
    let member_type = &args.member_type;

    Ok(quote! {
        #[middleware(modo::axum::middleware::from_fn(
            modo_tenant::guard::require_roles::<#tenant_type, #member_type>(&[#(#role_strs),*])
        ))]
        #func
    })
}

pub fn expand_deny_roles(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args: RolesArgs = syn::parse2(attr)?;
    let func: ItemFn = syn::parse2(item)?;
    let role_strs: Vec<&LitStr> = args.roles.iter().collect();
    let tenant_type = &args.tenant_type;
    let member_type = &args.member_type;

    Ok(quote! {
        #[middleware(modo::axum::middleware::from_fn(
            modo_tenant::guard::exclude_roles::<#tenant_type, #member_type>(&[#(#role_strs),*])
        ))]
        #func
    })
}
