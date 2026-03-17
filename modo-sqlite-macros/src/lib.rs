use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use std::collections::HashSet;
use std::path::PathBuf;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token};

struct EmbedMigrationsInput {
    path: Option<String>,
    group: Option<String>,
}

impl Parse for EmbedMigrationsInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut path = None;
        let mut group = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let value: LitStr = input.parse()?;

            match key.to_string().as_str() {
                "path" => path = Some(value.value()),
                "group" => group = Some(value.value()),
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown argument: {other}"),
                    ));
                }
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(EmbedMigrationsInput { path, group })
    }
}

/// Embed SQL migration files from a directory at compile time.
///
/// Scans `CARGO_MANIFEST_DIR/migrations/*.sql` by default.
/// Each file must be named `{YYYYMMDDHHmmss}_{description}.sql`.
///
/// # Usage
/// ```ignore
/// modo_sqlite::embed_migrations!();
/// modo_sqlite::embed_migrations!(path = "db/migrations", group = "jobs");
/// ```
#[proc_macro]
pub fn embed_migrations(input: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(input as EmbedMigrationsInput);

    let migrations_path = args.path.unwrap_or_else(|| "migrations".to_string());
    let group = args.group.unwrap_or_else(|| "default".to_string());

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let dir = PathBuf::from(&manifest_dir).join(&migrations_path);

    // If directory doesn't exist, emit nothing (no migrations)
    if !dir.exists() {
        return TokenStream::new();
    }

    // Read and filter .sql files
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to read migrations directory {}: {e}", dir.display()))
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "sql"))
        .collect();

    // Sort by filename for deterministic order
    entries.sort_by_key(|e| e.file_name());

    let mut seen_versions = HashSet::new();
    let mut submissions = Vec::new();

    for entry in entries {
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy().to_string();
        let stem = entry
            .path()
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();

        // Parse: {14-digit-timestamp}_{description}
        let underscore_pos = match stem.find('_') {
            Some(pos) if pos == 14 => pos,
            Some(pos) => {
                return syn::Error::new(
                    Span::call_site(),
                    format!(
                        "migration filename '{file_name_str}': timestamp must be exactly \
                         14 digits, found {pos} characters before first underscore"
                    ),
                )
                .to_compile_error()
                .into();
            }
            None => {
                return syn::Error::new(
                    Span::call_site(),
                    format!(
                        "migration filename '{file_name_str}': missing '_' separator \
                         after timestamp"
                    ),
                )
                .to_compile_error()
                .into();
            }
        };

        let timestamp_str = &stem[..underscore_pos];
        let description = &stem[underscore_pos + 1..];

        // Validate timestamp is all digits
        if !timestamp_str.chars().all(|c| c.is_ascii_digit()) {
            return syn::Error::new(
                Span::call_site(),
                format!(
                    "migration filename '{file_name_str}': timestamp '{timestamp_str}' \
                     contains non-numeric characters"
                ),
            )
            .to_compile_error()
            .into();
        }

        let version: u64 = timestamp_str.parse().unwrap();

        // Check for duplicates
        if !seen_versions.insert(version) {
            return syn::Error::new(
                Span::call_site(),
                format!("duplicate migration version: {version}"),
            )
            .to_compile_error()
            .into();
        }

        // Use include_str!() so rustc tracks the file as a dependency
        let sql_path_str = entry
            .path()
            .canonicalize()
            .unwrap_or_else(|e| panic!("failed to canonicalize {}: {e}", entry.path().display()))
            .to_string_lossy()
            .to_string();

        submissions.push(quote! {
            ::modo_sqlite::inventory::submit! {
                ::modo_sqlite::MigrationRegistration {
                    version: #version,
                    description: #description,
                    group: #group,
                    sql: include_str!(#sql_path_str),
                }
            }
        });
    }

    let expanded = quote! { #(#submissions)* };
    expanded.into()
}
