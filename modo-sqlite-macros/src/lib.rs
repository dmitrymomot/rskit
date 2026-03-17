use proc_macro::TokenStream;

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
pub fn embed_migrations(_input: TokenStream) -> TokenStream {
    TokenStream::new() // stub — implemented in Task 7
}
