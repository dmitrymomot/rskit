use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Fields, Ident, ItemStruct, Lit, LitStr, Result, Token, Type, parse2};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_uppercase() && i > 0 {
            let prev_upper = chars[i - 1].is_uppercase();
            let next_lower = chars.get(i + 1).is_some_and(|c| c.is_lowercase());
            if !prev_upper || next_lower {
                result.push('_');
            }
        }
        result.push(ch.to_ascii_lowercase());
    }
    result
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn validate_fk_action(action: &str) -> Result<()> {
    match action {
        "Cascade" | "SetNull" | "Restrict" | "NoAction" | "SetDefault" => Ok(()),
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "unsupported FK action: {action}. Expected one of: Cascade, SetNull, Restrict, NoAction, SetDefault"
            ),
        )),
    }
}

/// Extract the outermost type name from a `syn::Type`.
/// For `Option<String>` returns `"Option"`, for `String` returns `"String"`, etc.
fn type_name_str(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(tp) => tp.path.segments.last().map(|seg| seg.ident.to_string()),
        _ => None,
    }
}

/// Check if a type is `Option<T>`.
fn is_option_type(ty: &Type) -> bool {
    type_name_str(ty).as_deref() == Some("Option")
}

// ---------------------------------------------------------------------------
// Parsing types
// ---------------------------------------------------------------------------

struct EntityArgs {
    table_name: String,
    group: Option<String>,
}

impl syn::parse::Parse for EntityArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut table_name = None;
        let mut group = None;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match ident.to_string().as_str() {
                "table" => {
                    let val: LitStr = input.parse()?;
                    table_name = Some(val.value());
                }
                "group" => {
                    let val: LitStr = input.parse()?;
                    group = Some(val.value());
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        format!("unknown entity attribute: {other}"),
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let table_name = table_name.ok_or_else(|| input.error("missing `table` argument"))?;
        Ok(EntityArgs { table_name, group })
    }
}

struct StructAttrs {
    timestamps: bool,
    soft_delete: bool,
    framework: bool,
    indices: Vec<CompositeIndex>,
}

struct CompositeIndex {
    columns: Vec<String>,
    unique: bool,
}

#[derive(Default)]
struct FieldAttrs {
    primary_key: bool,
    auto_increment: Option<bool>,
    unique: bool,
    indexed: bool,
    column_type: Option<String>,
    default_value: Option<Lit>,
    default_expr: Option<String>,
    belongs_to: Option<String>,
    has_many: bool,
    has_one: bool,
    on_delete: Option<String>,
    on_update: Option<String>,
    via: Option<String>,
    renamed_from: Option<String>,
    auto: Option<String>,
}

enum FieldKind {
    Column,
    RelationOnly,
}

struct ParsedField {
    name: Ident,
    ty: Type,
    attrs: FieldAttrs,
    kind: FieldKind,
}

// ---------------------------------------------------------------------------
// Struct-level attribute parsing
// ---------------------------------------------------------------------------

fn parse_struct_attrs(input: &mut ItemStruct) -> Result<StructAttrs> {
    let mut timestamps = false;
    let mut soft_delete = false;
    let mut framework = false;
    let mut indices = Vec::new();
    let mut parse_errors: Vec<syn::Error> = Vec::new();

    input.attrs.retain(|attr| {
        if !attr.path().is_ident("entity") {
            return true;
        }

        if let Err(e) = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("timestamps") {
                timestamps = true;
                Ok(())
            } else if meta.path.is_ident("soft_delete") {
                soft_delete = true;
                Ok(())
            } else if meta.path.is_ident("framework") {
                framework = true;
                Ok(())
            } else if meta.path.is_ident("index") {
                let mut columns = Vec::new();
                let mut unique = false;

                meta.parse_nested_meta(|inner| {
                    if inner.path.is_ident("columns") {
                        let value = inner.value()?;
                        let content;
                        syn::bracketed!(content in value);
                        while !content.is_empty() {
                            let lit: LitStr = content.parse()?;
                            columns.push(lit.value());
                            if content.peek(Token![,]) {
                                content.parse::<Token![,]>()?;
                            }
                        }
                        Ok(())
                    } else if inner.path.is_ident("unique") {
                        unique = true;
                        Ok(())
                    } else {
                        Err(inner.error("expected `columns` or `unique`"))
                    }
                })?;

                indices.push(CompositeIndex { columns, unique });
                Ok(())
            } else {
                Err(meta
                    .error("expected `timestamps`, `soft_delete`, `framework`, or `index(...)`"))
            }
        }) {
            parse_errors.push(e);
        }

        false
    });

    if let Some(first) = parse_errors.into_iter().reduce(|mut a, b| {
        a.combine(b);
        a
    }) {
        return Err(first);
    }

    Ok(StructAttrs {
        timestamps,
        soft_delete,
        framework,
        indices,
    })
}

// ---------------------------------------------------------------------------
// Field-level attribute parsing
// ---------------------------------------------------------------------------

fn parse_field_attrs(field: &mut syn::Field) -> Result<FieldAttrs> {
    let mut attrs = FieldAttrs::default();
    let mut parse_errors: Vec<syn::Error> = Vec::new();

    field.attrs.retain(|attr| {
        if !attr.path().is_ident("entity") {
            return true;
        }

        if let Err(e) = attr.parse_nested_meta(|meta| {
            let name = meta
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();

            match name.as_str() {
                "primary_key" => attrs.primary_key = true,
                "auto_increment" => {
                    let lit: syn::LitBool = meta.value()?.parse()?;
                    attrs.auto_increment = Some(lit.value);
                }
                "unique" => attrs.unique = true,
                "indexed" => attrs.indexed = true,
                // Accepted but unused — Option<T> already implies nullable in SeaORM.
                "nullable" => {}
                "column_type" => {
                    let lit: LitStr = meta.value()?.parse()?;
                    attrs.column_type = Some(lit.value());
                }
                "default_value" => {
                    let lit: Lit = meta.value()?.parse()?;
                    attrs.default_value = Some(lit);
                }
                "default_expr" => {
                    let lit: LitStr = meta.value()?.parse()?;
                    attrs.default_expr = Some(lit.value());
                }
                "belongs_to" => {
                    let lit: LitStr = meta.value()?.parse()?;
                    attrs.belongs_to = Some(lit.value());
                }
                "has_many" => attrs.has_many = true,
                "has_one" => attrs.has_one = true,
                "on_delete" => {
                    let lit: LitStr = meta.value()?.parse()?;
                    attrs.on_delete = Some(lit.value());
                }
                "on_update" => {
                    let lit: LitStr = meta.value()?.parse()?;
                    attrs.on_update = Some(lit.value());
                }
                "via" => {
                    let lit: LitStr = meta.value()?.parse()?;
                    attrs.via = Some(lit.value());
                }
                "renamed_from" => {
                    let lit: LitStr = meta.value()?.parse()?;
                    attrs.renamed_from = Some(lit.value());
                }
                "auto" => {
                    let lit: LitStr = meta.value()?.parse()?;
                    let val = lit.value();
                    if val != "ulid" && val != "nanoid" {
                        return Err(meta.error("auto must be \"ulid\" or \"nanoid\""));
                    }
                    attrs.auto = Some(val);
                }
                other => {
                    return Err(meta.error(format!("unknown entity field attribute: {other}")));
                }
            }
            Ok(())
        }) {
            parse_errors.push(e);
        }

        false
    });

    if let Some(first) = parse_errors.into_iter().reduce(|mut a, b| {
        a.combine(b);
        a
    }) {
        return Err(first);
    }

    Ok(attrs)
}

// ---------------------------------------------------------------------------
// Default value generation
// ---------------------------------------------------------------------------

/// Generate a default-value expression for a field based on its type and attributes.
fn default_expr_for_field(f: &ParsedField) -> TokenStream {
    // Auto-ID fields
    if let Some(ref strategy) = f.attrs.auto {
        return match strategy.as_str() {
            "ulid" => quote! { modo_db::generate_ulid() },
            "nanoid" => quote! { modo_db::generate_nanoid() },
            _ => unreachable!(),
        };
    }

    // Option<T> => None
    if is_option_type(&f.ty) {
        return quote! { None };
    }

    // Type-based defaults
    match type_name_str(&f.ty).as_deref() {
        Some("String") => quote! { String::new() },
        Some("bool") => quote! { false },
        Some(
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
            | "usize",
        ) => quote! { 0 },
        Some("f32" | "f64") => quote! { 0.0 },
        _ => quote! { Default::default() },
    }
}

// ---------------------------------------------------------------------------
// Code generation
// ---------------------------------------------------------------------------

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args: EntityArgs = parse2(attr)?;
    let mut input: ItemStruct = parse2(item)?;

    let struct_name = input.ident.clone();
    let mod_name = format_ident!("{}", to_snake_case(&struct_name.to_string()));
    let table_name = &args.table_name;
    let group_str = args.group.as_deref().unwrap_or("default");

    let struct_attrs = parse_struct_attrs(&mut input)?;

    let fields = match &mut input.fields {
        Fields::Named(f) => &mut f.named,
        _ => {
            return Err(syn::Error::new_spanned(
                &input,
                "#[modo::entity] requires a struct with named fields",
            ));
        }
    };

    if fields.is_empty() {
        return Err(syn::Error::new_spanned(
            &input,
            "#[modo::entity] requires at least one field",
        ));
    }

    if struct_attrs.timestamps {
        for field in fields.iter() {
            if let Some(ref name) = field.ident {
                let s = name.to_string();
                if s == "created_at" || s == "updated_at" {
                    return Err(syn::Error::new_spanned(
                        name,
                        format!(
                            "field `{s}` conflicts with #[entity(timestamps)] — remove it or remove the timestamps attribute"
                        ),
                    ));
                }
            }
        }
    }
    if struct_attrs.soft_delete {
        for field in fields.iter() {
            if let Some(ref name) = field.ident
                && name == "deleted_at"
            {
                return Err(syn::Error::new_spanned(
                    name,
                    "field `deleted_at` conflicts with #[entity(soft_delete)] — remove it or remove the soft_delete attribute",
                ));
            }
        }
    }

    let mut parsed_fields = Vec::new();
    for field in fields.iter_mut() {
        let name = field.ident.clone().unwrap();
        let ty = field.ty.clone();
        let attrs = parse_field_attrs(field)?;

        let kind = if (attrs.has_many || attrs.has_one) && attrs.belongs_to.is_none() {
            FieldKind::RelationOnly
        } else {
            FieldKind::Column
        };

        parsed_fields.push(ParsedField {
            name,
            ty,
            attrs,
            kind,
        });
    }

    let pk_count = parsed_fields.iter().filter(|f| f.attrs.primary_key).count();

    for f in &parsed_fields {
        if f.attrs.auto.is_some() && !f.attrs.primary_key {
            return Err(syn::Error::new_spanned(
                &f.name,
                "#[entity(auto = \"...\")] can only be used on primary_key fields",
            ));
        }
    }

    // =========================================================================
    // 1. SeaORM model fields (same logic as before)
    // =========================================================================

    let mut model_fields = Vec::new();
    for f in &parsed_fields {
        if matches!(f.kind, FieldKind::RelationOnly) {
            continue;
        }

        let name = &f.name;
        let ty = &f.ty;
        let mut sea_orm_attrs = Vec::new();

        if f.attrs.primary_key {
            let auto_inc = if f.attrs.auto.is_some() {
                false
            } else {
                f.attrs.auto_increment.unwrap_or(pk_count <= 1)
            };
            if !auto_inc {
                sea_orm_attrs.push(quote! { primary_key, auto_increment = false });
            } else {
                sea_orm_attrs.push(quote! { primary_key });
            }
        }

        if f.attrs.unique {
            sea_orm_attrs.push(quote! { unique });
        }

        if f.attrs.indexed {
            sea_orm_attrs.push(quote! { indexed });
        }

        if let Some(ref ct) = f.attrs.column_type {
            sea_orm_attrs.push(quote! { column_type = #ct });
        }

        if let Some(ref lit) = f.attrs.default_value {
            sea_orm_attrs.push(quote! { default_value = #lit });
        }

        if let Some(ref expr) = f.attrs.default_expr {
            sea_orm_attrs.push(quote! { default_expr = #expr });
        }

        if let Some(ref old_name) = f.attrs.renamed_from {
            let comment = format!("renamed_from \"{old_name}\"");
            sea_orm_attrs.push(quote! { comment = #comment });
        }

        let sea_orm_attr = if sea_orm_attrs.is_empty() {
            quote! {}
        } else {
            quote! { #[sea_orm(#(#sea_orm_attrs),*)] }
        };

        model_fields.push(quote! {
            #sea_orm_attr
            pub #name: #ty,
        });
    }

    // Timestamp and soft-delete model fields
    if struct_attrs.timestamps {
        model_fields.push(quote! {
            pub created_at: modo_db::chrono::DateTime<modo_db::chrono::Utc>,
        });
        model_fields.push(quote! {
            pub updated_at: modo_db::chrono::DateTime<modo_db::chrono::Utc>,
        });
    }

    if struct_attrs.soft_delete {
        model_fields.push(quote! {
            pub deleted_at: Option<modo_db::chrono::DateTime<modo_db::chrono::Utc>>,
        });
    }

    // =========================================================================
    // 2. Relations (same logic as before)
    // =========================================================================

    let mut relation_variants = Vec::new();
    let mut related_impls = Vec::new();

    for f in &parsed_fields {
        if let Some(ref target) = f.attrs.belongs_to {
            let target_mod = format_ident!("{}", to_snake_case(target));
            let variant_name = format_ident!("{target}");
            let from_col_str = format!("Column::{}", to_pascal_case(&f.name.to_string()));
            let to_col_str = format!("super::{target_mod}::Column::Id");
            let belongs_to_str = format!("super::{target_mod}::Entity");

            let mut rel_attrs = vec![
                quote! { belongs_to = #belongs_to_str },
                quote! { from = #from_col_str },
                quote! { to = #to_col_str },
            ];

            if let Some(ref action) = f.attrs.on_delete {
                validate_fk_action(action)?;
                rel_attrs.push(quote! { on_delete = #action });
            }
            if let Some(ref action) = f.attrs.on_update {
                validate_fk_action(action)?;
                rel_attrs.push(quote! { on_update = #action });
            }

            relation_variants.push(quote! {
                #[sea_orm(#(#rel_attrs),*)]
                #variant_name,
            });

            related_impls.push(quote! {
                impl modo_db::sea_orm::entity::prelude::Related<super::#target_mod::Entity> for Entity {
                    fn to() -> modo_db::sea_orm::entity::prelude::RelationDef {
                        Relation::#variant_name.def()
                    }
                }
            });
        }
    }

    for f in &parsed_fields {
        if !matches!(f.kind, FieldKind::RelationOnly) {
            continue;
        }

        let pascal = to_pascal_case(&f.name.to_string());
        let target = if f.attrs.has_many {
            pascal.trim_end_matches('s').to_string()
        } else {
            pascal
        };

        let target_mod = format_ident!("{}", to_snake_case(&target));

        if let Some(ref via) = f.attrs.via {
            let via_mod = format_ident!("{}", to_snake_case(via));
            let self_variant = format_ident!("{struct_name}");
            let target_variant = format_ident!("{target}");

            related_impls.push(quote! {
                impl modo_db::sea_orm::entity::prelude::Related<super::#target_mod::Entity> for Entity {
                    fn to() -> modo_db::sea_orm::entity::prelude::RelationDef {
                        super::#via_mod::Relation::#target_variant.def()
                    }
                    fn via() -> Option<modo_db::sea_orm::entity::prelude::RelationDef> {
                        Some(super::#via_mod::Relation::#self_variant.def().rev())
                    }
                }
            });
        } else {
            let self_variant = format_ident!("{struct_name}");

            related_impls.push(quote! {
                impl modo_db::sea_orm::entity::prelude::Related<super::#target_mod::Entity> for Entity {
                    fn to() -> modo_db::sea_orm::entity::prelude::RelationDef {
                        super::#target_mod::Relation::#self_variant.def().rev()
                    }
                }
            });
        }
    }

    // =========================================================================
    // 3. Relation enum
    // =========================================================================

    let relation_enum = if relation_variants.is_empty() {
        quote! {
            #[derive(Copy, Clone, Debug, modo_db::sea_orm::EnumIter, modo_db::sea_orm::DeriveRelation)]
            pub enum Relation {}
        }
    } else {
        quote! {
            #[derive(Copy, Clone, Debug, modo_db::sea_orm::EnumIter, modo_db::sea_orm::DeriveRelation)]
            pub enum Relation {
                #(#relation_variants)*
            }
        }
    };

    // =========================================================================
    // 4. Extra SQL for indices
    // =========================================================================

    let mut extra_sql_stmts = Vec::new();
    for idx in &struct_attrs.indices {
        let cols = idx.columns.join(", ");
        let col_names = idx.columns.join("_");
        let idx_name = format!("idx_{table_name}_{col_names}");
        let sql = if idx.unique {
            format!("CREATE UNIQUE INDEX IF NOT EXISTS {idx_name} ON {table_name}({cols})")
        } else {
            format!("CREATE INDEX IF NOT EXISTS {idx_name} ON {table_name}({cols})")
        };
        extra_sql_stmts.push(sql);
    }

    // Add deleted_at index for soft-delete entities
    if struct_attrs.soft_delete {
        let idx_name = format!("idx_{table_name}_deleted_at");
        extra_sql_stmts.push(format!(
            "CREATE INDEX IF NOT EXISTS {idx_name} ON {table_name}(deleted_at)"
        ));
    }

    let is_framework = struct_attrs.framework;

    let extra_sql_tokens = if extra_sql_stmts.is_empty() {
        quote! { &[] }
    } else {
        quote! { &[#(#extra_sql_stmts),*] }
    };

    // =========================================================================
    // 5. Preserved user struct
    // =========================================================================

    // Collect column fields (not relation-only) for the domain struct
    let column_fields: Vec<&ParsedField> = parsed_fields
        .iter()
        .filter(|f| matches!(f.kind, FieldKind::Column))
        .collect();

    let struct_field_defs: Vec<TokenStream> = column_fields
        .iter()
        .map(|f| {
            let name = &f.name;
            let ty = &f.ty;
            quote! { pub #name: #ty, }
        })
        .collect();

    let timestamp_struct_fields = if struct_attrs.timestamps {
        quote! {
            pub created_at: modo_db::chrono::DateTime<modo_db::chrono::Utc>,
            pub updated_at: modo_db::chrono::DateTime<modo_db::chrono::Utc>,
        }
    } else {
        quote! {}
    };

    let soft_delete_struct_field = if struct_attrs.soft_delete {
        quote! {
            pub deleted_at: Option<modo_db::chrono::DateTime<modo_db::chrono::Utc>>,
        }
    } else {
        quote! {}
    };

    let vis = &input.vis;
    let preserved_struct = quote! {
        #[derive(Clone, Debug, serde::Serialize)]
        #vis struct #struct_name {
            #(#struct_field_defs)*
            #timestamp_struct_fields
            #soft_delete_struct_field
        }
    };

    // =========================================================================
    // 6. Default impl
    // =========================================================================

    let mut default_field_stmts: Vec<TokenStream> = column_fields
        .iter()
        .map(|f| {
            let name = &f.name;
            let expr = default_expr_for_field(f);
            quote! { #name: #expr, }
        })
        .collect();

    if struct_attrs.timestamps {
        default_field_stmts.push(quote! { created_at: modo_db::chrono::Utc::now(), });
        default_field_stmts.push(quote! { updated_at: modo_db::chrono::Utc::now(), });
    }

    if struct_attrs.soft_delete {
        default_field_stmts.push(quote! { deleted_at: None, });
    }

    let default_impl = quote! {
        impl Default for #struct_name {
            fn default() -> Self {
                Self {
                    #(#default_field_stmts)*
                }
            }
        }
    };

    // =========================================================================
    // 7. From<Model> impl
    // =========================================================================

    let mut from_field_stmts: Vec<TokenStream> = column_fields
        .iter()
        .map(|f| {
            let name = &f.name;
            quote! { #name: model.#name, }
        })
        .collect();

    if struct_attrs.timestamps {
        from_field_stmts.push(quote! { created_at: model.created_at, });
        from_field_stmts.push(quote! { updated_at: model.updated_at, });
    }

    if struct_attrs.soft_delete {
        from_field_stmts.push(quote! { deleted_at: model.deleted_at, });
    }

    let from_model_impl = quote! {
        impl From<#mod_name::Model> for #struct_name {
            fn from(model: #mod_name::Model) -> Self {
                Self {
                    #(#from_field_stmts)*
                }
            }
        }
    };

    // =========================================================================
    // 8. Record impl
    // =========================================================================

    // into_active_model_full: set ALL column fields + timestamps + soft_delete
    let mut am_full_stmts: Vec<TokenStream> = column_fields
        .iter()
        .map(|f| {
            let name = &f.name;
            quote! { #name: modo_db::sea_orm::ActiveValue::Set(self.#name.clone()), }
        })
        .collect();

    if struct_attrs.timestamps {
        am_full_stmts
            .push(quote! { created_at: modo_db::sea_orm::ActiveValue::Set(self.created_at), });
        am_full_stmts
            .push(quote! { updated_at: modo_db::sea_orm::ActiveValue::Set(self.updated_at), });
    }

    if struct_attrs.soft_delete {
        am_full_stmts
            .push(quote! { deleted_at: modo_db::sea_orm::ActiveValue::Set(self.deleted_at), });
    }

    // into_active_model: set only PK fields, rest use Default (NotSet)
    let am_pk_stmts: Vec<TokenStream> = parsed_fields
        .iter()
        .filter(|f| f.attrs.primary_key)
        .map(|f| {
            let name = &f.name;
            quote! { #name: modo_db::sea_orm::ActiveValue::Set(self.#name.clone()), }
        })
        .collect();

    // apply_auto_fields: handle auto-ID and timestamps
    let auto_field_stmts: Vec<TokenStream> = parsed_fields
        .iter()
        .filter_map(|f| {
            f.attrs.auto.as_ref().map(|strategy| {
                let name = &f.name;
                let gen_call = match strategy.as_str() {
                    "ulid" => quote! { modo_db::generate_ulid() },
                    "nanoid" => quote! { modo_db::generate_nanoid() },
                    _ => unreachable!(),
                };
                quote! {
                    if is_insert {
                        if let modo_db::sea_orm::ActiveValue::Set(ref id) = am.#name {
                            if id.is_empty() {
                                am.#name = modo_db::sea_orm::ActiveValue::Set(#gen_call);
                            }
                        } else {
                            am.#name = modo_db::sea_orm::ActiveValue::Set(#gen_call);
                        }
                    }
                }
            })
        })
        .collect();

    let timestamp_auto_stmts = if struct_attrs.timestamps {
        quote! {
            let now = modo_db::chrono::Utc::now();
            if is_insert {
                am.created_at = modo_db::sea_orm::ActiveValue::Set(now);
            }
            am.updated_at = modo_db::sea_orm::ActiveValue::Set(now);
        }
    } else {
        quote! {}
    };

    // Determine PK types for find_by_id / delete_by_id signatures
    let pk_fields: Vec<&ParsedField> = parsed_fields
        .iter()
        .filter(|f| f.attrs.primary_key)
        .collect();

    // Generate find_by_id and delete_by_id based on PK configuration
    let (find_by_id_method, delete_by_id_method) =
        gen_pk_methods(&pk_fields, &mod_name, &struct_attrs);

    // CRUD methods: insert, update, delete
    let delete_method = if struct_attrs.soft_delete {
        // Soft-delete: set deleted_at = now instead of real delete
        let update_stmts = if struct_attrs.timestamps {
            quote! {
                let now = modo_db::chrono::Utc::now();
                self.deleted_at = Some(now);
                self.updated_at = now;
            }
        } else {
            quote! {
                self.deleted_at = Some(modo_db::chrono::Utc::now());
            }
        };

        quote! {
            pub async fn delete(mut self, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<(), modo::Error> {
                use modo_db::DefaultHooks;
                self.before_delete()?;
                #update_stmts
                let mut am = <Self as modo_db::Record>::into_active_model_full(&self);
                <Self as modo_db::Record>::apply_auto_fields(&mut am, false);
                use modo_db::sea_orm::ActiveModelTrait;
                am.update(db).await.map_err(modo_db::db_err_to_error)?;
                Ok(())
            }
        }
    } else {
        quote! {
            pub async fn delete(self, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<(), modo::Error> {
                use modo_db::DefaultHooks;
                self.before_delete()?;
                modo_db::do_delete(self, db).await
            }
        }
    };

    // Override find_all and query for soft-delete to filter out deleted records
    let find_all_override = if struct_attrs.soft_delete {
        quote! {
            fn find_all(
                db: &impl modo_db::sea_orm::ConnectionTrait,
            ) -> impl std::future::Future<Output = Result<Vec<Self>, modo::Error>> + Send {
                async {
                    use modo_db::sea_orm::EntityTrait as _;
                    use modo_db::sea_orm::QueryFilter;
                    use modo_db::sea_orm::ColumnTrait;
                    let models = #mod_name::Entity::find()
                        .filter(#mod_name::Column::DeletedAt.is_null())
                        .all(db)
                        .await
                        .map_err(modo_db::db_err_to_error)?;
                    Ok(models.into_iter().map(Self::from_model).collect())
                }
            }
        }
    } else {
        quote! {}
    };

    let query_override = if struct_attrs.soft_delete {
        quote! {
            fn query() -> modo_db::EntityQuery<Self, #mod_name::Entity> {
                use modo_db::sea_orm::EntityTrait as _;
                use modo_db::sea_orm::QueryFilter;
                use modo_db::sea_orm::ColumnTrait;
                modo_db::EntityQuery::new(
                    #mod_name::Entity::find().filter(#mod_name::Column::DeletedAt.is_null())
                )
            }
        }
    } else {
        quote! {}
    };

    let record_impl = quote! {
        impl modo_db::Record for #struct_name {
            type Entity = #mod_name::Entity;
            type ActiveModel = #mod_name::ActiveModel;

            fn from_model(model: <#mod_name::Entity as modo_db::sea_orm::EntityTrait>::Model) -> Self {
                Self::from(model)
            }

            fn into_active_model_full(&self) -> #mod_name::ActiveModel {
                #mod_name::ActiveModel {
                    #(#am_full_stmts)*
                }
            }

            fn into_active_model(&self) -> #mod_name::ActiveModel {
                #mod_name::ActiveModel {
                    #(#am_pk_stmts)*
                    ..Default::default()
                }
            }

            fn apply_auto_fields(am: &mut #mod_name::ActiveModel, is_insert: bool) {
                #(#auto_field_stmts)*
                #timestamp_auto_stmts
            }

            #find_all_override

            #query_override
        }
    };

    // CRUD methods as inherent methods on the struct (not trait methods)
    let crud_impl = quote! {
        impl #struct_name {
            #find_by_id_method

            #delete_by_id_method

            pub async fn insert(mut self, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<Self, modo::Error> {
                use modo_db::DefaultHooks;
                self.before_save()?;
                let result = modo_db::do_insert(self, db).await?;
                result.after_save()?;
                Ok(result)
            }

            pub async fn update(&mut self, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<(), modo::Error> {
                use modo_db::DefaultHooks;
                self.before_save()?;
                let refreshed = modo_db::do_update(self, db).await?;
                *self = refreshed;
                self.after_save()?;
                Ok(())
            }

            #delete_method
        }
    };

    // =========================================================================
    // 9. Relation accessor methods
    // =========================================================================

    let mut relation_accessors = Vec::new();

    for f in &parsed_fields {
        if let Some(ref target) = f.attrs.belongs_to {
            // belongs_to accessor: field `user_id` -> method `user()`
            let fk_field_name = f.name.to_string();
            let method_name_str = fk_field_name.strip_suffix("_id").unwrap_or(&fk_field_name);
            let method_name = format_ident!("{method_name_str}");
            let target_ident = format_ident!("{target}");
            let target_mod = format_ident!("{}", to_snake_case(target));
            let fk_field = &f.name;

            let is_string_fk = type_name_str(&f.ty).as_deref() == Some("String");
            let accessor = if is_string_fk {
                quote! {
                    pub async fn #method_name(&self, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<Option<#target_ident>, modo::Error> {
                        use modo_db::sea_orm::EntityTrait;
                        #target_mod::Entity::find_by_id(&self.#fk_field)
                            .one(db)
                            .await
                            .map_err(modo_db::db_err_to_error)
                            .map(|opt| opt.map(#target_ident::from))
                    }
                }
            } else {
                quote! {
                    pub async fn #method_name(&self, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<Option<#target_ident>, modo::Error> {
                        use modo_db::sea_orm::EntityTrait;
                        #target_mod::Entity::find_by_id(self.#fk_field.clone())
                            .one(db)
                            .await
                            .map_err(modo_db::db_err_to_error)
                            .map(|opt| opt.map(#target_ident::from))
                    }
                }
            };

            relation_accessors.push(accessor);
        }
    }

    for f in &parsed_fields {
        if !matches!(f.kind, FieldKind::RelationOnly) {
            continue;
        }

        let field_name = &f.name;
        let pascal = to_pascal_case(&field_name.to_string());
        let target = if f.attrs.has_many {
            pascal.trim_end_matches('s').to_string()
        } else {
            pascal.clone()
        };

        let target_ident = format_ident!("{target}");
        let target_mod = format_ident!("{}", to_snake_case(&target));

        // The FK column on the target table: {snake_case(struct_name)}_id
        let fk_col_name = format!("{}_id", to_snake_case(&struct_name.to_string()));
        let fk_col_pascal = format_ident!("{}", to_pascal_case(&fk_col_name));

        if f.attrs.has_many {
            let accessor = quote! {
                pub async fn #field_name(&self, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<Vec<#target_ident>, modo::Error> {
                    use modo_db::sea_orm::EntityTrait;
                    use modo_db::sea_orm::QueryFilter;
                    use modo_db::sea_orm::ColumnTrait;
                    #target_mod::Entity::find()
                        .filter(#target_mod::Column::#fk_col_pascal.eq(&self.id))
                        .all(db)
                        .await
                        .map_err(modo_db::db_err_to_error)
                        .map(|models| models.into_iter().map(#target_ident::from).collect())
                }
            };
            relation_accessors.push(accessor);
        } else if f.attrs.has_one {
            let accessor = quote! {
                pub async fn #field_name(&self, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<Option<#target_ident>, modo::Error> {
                    use modo_db::sea_orm::EntityTrait;
                    use modo_db::sea_orm::QueryFilter;
                    use modo_db::sea_orm::ColumnTrait;
                    #target_mod::Entity::find()
                        .filter(#target_mod::Column::#fk_col_pascal.eq(&self.id))
                        .one(db)
                        .await
                        .map_err(modo_db::db_err_to_error)
                        .map(|opt| opt.map(#target_ident::from))
                }
            };
            relation_accessors.push(accessor);
        }
    }

    let relation_accessor_impl = if relation_accessors.is_empty() {
        quote! {}
    } else {
        quote! {
            impl #struct_name {
                #(#relation_accessors)*
            }
        }
    };

    // =========================================================================
    // 10. Soft-delete extra methods on the struct
    // =========================================================================

    let soft_delete_methods = if struct_attrs.soft_delete {
        let force_delete_by_id_method = gen_force_delete_by_id(&pk_fields, &mod_name);

        quote! {
            impl #struct_name {
                /// Query that includes soft-deleted records.
                pub fn with_deleted() -> modo_db::EntityQuery<Self, #mod_name::Entity> {
                    use modo_db::sea_orm::EntityTrait as _;
                    modo_db::EntityQuery::new(#mod_name::Entity::find())
                }

                /// Query that returns only soft-deleted records.
                pub fn only_deleted() -> modo_db::EntityQuery<Self, #mod_name::Entity> {
                    use modo_db::sea_orm::EntityTrait as _;
                    use modo_db::sea_orm::QueryFilter;
                    use modo_db::sea_orm::ColumnTrait;
                    modo_db::EntityQuery::new(
                        #mod_name::Entity::find().filter(#mod_name::Column::DeletedAt.is_not_null())
                    )
                }

                /// Restore a soft-deleted record by clearing `deleted_at`.
                pub async fn restore(&mut self, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<(), modo::Error> {
                    self.deleted_at = None;
                    let mut am = <Self as modo_db::Record>::into_active_model_full(self);
                    <Self as modo_db::Record>::apply_auto_fields(&mut am, false);
                    use modo_db::sea_orm::ActiveModelTrait;
                    let model = am.update(db).await.map_err(modo_db::db_err_to_error)?;
                    *self = Self::from(model);
                    Ok(())
                }

                /// Permanently delete this record from the database (hard delete).
                pub async fn force_delete(self, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<(), modo::Error> {
                    modo_db::do_delete(self, db).await
                }

                #force_delete_by_id_method

                /// Bulk hard-delete builder (bypasses soft-delete).
                pub fn force_delete_many() -> modo_db::EntityDeleteMany<#mod_name::Entity> {
                    use modo_db::sea_orm::EntityTrait as _;
                    modo_db::EntityDeleteMany::new(#mod_name::Entity::delete_many())
                }
            }
        }
    } else {
        quote! {}
    };

    // =========================================================================
    // Assemble final output
    // =========================================================================

    // ActiveModelBehavior is now always empty -- auto-ID and timestamps
    // are handled by Record::apply_auto_fields
    let active_model_behavior = quote! {
        impl ActiveModelBehavior for ActiveModel {}
    };

    Ok(quote! {
        // 1. Preserved user struct
        #preserved_struct

        // 2. SeaORM module
        pub mod #mod_name {
            use modo_db::sea_orm;
            use modo_db::sea_orm::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, modo_db::sea_orm::DeriveEntityModel)]
            #[sea_orm(table_name = #table_name)]
            pub struct Model {
                #(#model_fields)*
            }

            #relation_enum

            #(#related_impls)*

            #active_model_behavior
        }

        // 3. Default impl
        #default_impl

        // 4. From<Model> impl
        #from_model_impl

        // 5. Record impl
        #record_impl

        // 6. CRUD methods (inherent)
        #crud_impl

        // 7. Relation accessors
        #relation_accessor_impl

        // 8. Soft-delete methods
        #soft_delete_methods

        // 9. Entity registration
        modo_db::inventory::submit! {
            modo_db::EntityRegistration {
                table_name: #table_name,
                group: #group_str,
                register_fn: |sb| sb.register(#mod_name::Entity),
                is_framework: #is_framework,
                extra_sql: #extra_sql_tokens,
            }
        }
    })
}

// ---------------------------------------------------------------------------
// PK-dependent method generation helpers
// ---------------------------------------------------------------------------

/// Generate `find_by_id` and `delete_by_id` method bodies based on PK configuration.
fn gen_pk_methods(
    pk_fields: &[&ParsedField],
    mod_name: &Ident,
    struct_attrs: &StructAttrs,
) -> (TokenStream, TokenStream) {
    if pk_fields.len() == 1 {
        let pk_field = pk_fields[0];
        let pk_ty = &pk_field.ty;
        let pk_name = &pk_field.name;
        let is_string_pk = type_name_str(pk_ty).as_deref() == Some("String");
        let pk_col_pascal = format_ident!("{}", to_pascal_case(&pk_name.to_string()));

        if is_string_pk {
            gen_string_pk_methods(mod_name, struct_attrs, &pk_col_pascal)
        } else {
            gen_typed_pk_methods(mod_name, struct_attrs, pk_ty, &pk_col_pascal)
        }
    } else {
        gen_composite_pk_methods(pk_fields, mod_name, struct_attrs)
    }
}

/// String PK: `find_by_id(id: &str, ...)` and `delete_by_id(id: &str, ...)`
fn gen_string_pk_methods(
    mod_name: &Ident,
    struct_attrs: &StructAttrs,
    pk_col_pascal: &Ident,
) -> (TokenStream, TokenStream) {
    let find_body = if struct_attrs.soft_delete {
        quote! {
            use modo_db::sea_orm::EntityTrait;
            use modo_db::sea_orm::QueryFilter;
            use modo_db::sea_orm::ColumnTrait;
            #mod_name::Entity::find_by_id(id)
                .filter(#mod_name::Column::DeletedAt.is_null())
                .one(db)
                .await
                .map_err(modo_db::db_err_to_error)?
                .map(Self::from)
                .ok_or_else(|| modo::Error::from(modo::HttpError::NotFound))
        }
    } else {
        quote! {
            use modo_db::sea_orm::EntityTrait;
            #mod_name::Entity::find_by_id(id)
                .one(db)
                .await
                .map_err(modo_db::db_err_to_error)?
                .map(Self::from)
                .ok_or_else(|| modo::Error::from(modo::HttpError::NotFound))
        }
    };

    let delete_body = if struct_attrs.soft_delete {
        quote! {
            use modo_db::sea_orm::EntityTrait;
            use modo_db::sea_orm::ColumnTrait;
            use modo_db::sea_orm::QueryFilter;
            let now = modo_db::chrono::Utc::now();
            let result = #mod_name::Entity::update_many()
                .filter(#mod_name::Column::#pk_col_pascal.eq(id))
                .filter(#mod_name::Column::DeletedAt.is_null())
                .col_expr(#mod_name::Column::DeletedAt, modo_db::sea_orm::sea_query::Expr::value(Some(now)))
                .exec(db)
                .await
                .map_err(modo_db::db_err_to_error)?;
            if result.rows_affected == 0 {
                return Err(modo::Error::from(modo::HttpError::NotFound));
            }
            Ok(())
        }
    } else {
        quote! {
            let record = Self::find_by_id(id, db).await?;
            record.delete(db).await
        }
    };

    (
        quote! {
            pub async fn find_by_id(id: &str, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<Self, modo::Error> {
                #find_body
            }
        },
        quote! {
            pub async fn delete_by_id(id: &str, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<(), modo::Error> {
                #delete_body
            }
        },
    )
}

/// Non-String single PK: `find_by_id(id: T, ...)` and `delete_by_id(id: T, ...)`
fn gen_typed_pk_methods(
    mod_name: &Ident,
    struct_attrs: &StructAttrs,
    pk_ty: &Type,
    pk_col_pascal: &Ident,
) -> (TokenStream, TokenStream) {
    let find_body = if struct_attrs.soft_delete {
        quote! {
            use modo_db::sea_orm::EntityTrait;
            use modo_db::sea_orm::QueryFilter;
            use modo_db::sea_orm::ColumnTrait;
            #mod_name::Entity::find_by_id(id)
                .filter(#mod_name::Column::DeletedAt.is_null())
                .one(db)
                .await
                .map_err(modo_db::db_err_to_error)?
                .map(Self::from)
                .ok_or_else(|| modo::Error::from(modo::HttpError::NotFound))
        }
    } else {
        quote! {
            use modo_db::sea_orm::EntityTrait;
            #mod_name::Entity::find_by_id(id)
                .one(db)
                .await
                .map_err(modo_db::db_err_to_error)?
                .map(Self::from)
                .ok_or_else(|| modo::Error::from(modo::HttpError::NotFound))
        }
    };

    let delete_body = if struct_attrs.soft_delete {
        quote! {
            use modo_db::sea_orm::EntityTrait;
            use modo_db::sea_orm::ColumnTrait;
            use modo_db::sea_orm::QueryFilter;
            let now = modo_db::chrono::Utc::now();
            let result = #mod_name::Entity::update_many()
                .filter(#mod_name::Column::#pk_col_pascal.eq(id))
                .filter(#mod_name::Column::DeletedAt.is_null())
                .col_expr(#mod_name::Column::DeletedAt, modo_db::sea_orm::sea_query::Expr::value(Some(now)))
                .exec(db)
                .await
                .map_err(modo_db::db_err_to_error)?;
            if result.rows_affected == 0 {
                return Err(modo::Error::from(modo::HttpError::NotFound));
            }
            Ok(())
        }
    } else {
        quote! {
            let record = Self::find_by_id(id, db).await?;
            record.delete(db).await
        }
    };

    (
        quote! {
            pub async fn find_by_id(id: #pk_ty, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<Self, modo::Error> {
                #find_body
            }
        },
        quote! {
            pub async fn delete_by_id(id: #pk_ty, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<(), modo::Error> {
                #delete_body
            }
        },
    )
}

/// Composite PK: `find_by_id(id: (T1, T2), ...)` and `delete_by_id(id: (T1, T2), ...)`
fn gen_composite_pk_methods(
    pk_fields: &[&ParsedField],
    mod_name: &Ident,
    struct_attrs: &StructAttrs,
) -> (TokenStream, TokenStream) {
    let pk_types: Vec<&Type> = pk_fields.iter().map(|f| &f.ty).collect();

    let find_body = if struct_attrs.soft_delete {
        quote! {
            use modo_db::sea_orm::EntityTrait;
            use modo_db::sea_orm::QueryFilter;
            use modo_db::sea_orm::ColumnTrait;
            #mod_name::Entity::find_by_id(id.clone())
                .filter(#mod_name::Column::DeletedAt.is_null())
                .one(db)
                .await
                .map_err(modo_db::db_err_to_error)?
                .map(Self::from)
                .ok_or_else(|| modo::Error::from(modo::HttpError::NotFound))
        }
    } else {
        quote! {
            use modo_db::sea_orm::EntityTrait;
            #mod_name::Entity::find_by_id(id.clone())
                .one(db)
                .await
                .map_err(modo_db::db_err_to_error)?
                .map(Self::from)
                .ok_or_else(|| modo::Error::from(modo::HttpError::NotFound))
        }
    };

    let delete_body = if struct_attrs.soft_delete {
        let pk_names: Vec<&Ident> = pk_fields.iter().map(|f| &f.name).collect();
        let pk_col_pascals: Vec<Ident> = pk_names
            .iter()
            .map(|n| format_ident!("{}", to_pascal_case(&n.to_string())))
            .collect();
        let pk_indices: Vec<syn::Index> = (0..pk_fields.len()).map(syn::Index::from).collect();

        quote! {
            use modo_db::sea_orm::EntityTrait;
            use modo_db::sea_orm::ColumnTrait;
            use modo_db::sea_orm::QueryFilter;
            let now = modo_db::chrono::Utc::now();
            let mut update = #mod_name::Entity::update_many();
            #(
                update = modo_db::sea_orm::QueryFilter::filter(
                    update,
                    #mod_name::Column::#pk_col_pascals.eq(id.#pk_indices.clone()),
                );
            )*
            let result = update
                .filter(#mod_name::Column::DeletedAt.is_null())
                .col_expr(#mod_name::Column::DeletedAt, modo_db::sea_orm::sea_query::Expr::value(Some(now)))
                .exec(db)
                .await
                .map_err(modo_db::db_err_to_error)?;
            if result.rows_affected == 0 {
                return Err(modo::Error::from(modo::HttpError::NotFound));
            }
            Ok(())
        }
    } else {
        quote! {
            let record = Self::find_by_id(id, db).await?;
            record.delete(db).await
        }
    };

    (
        quote! {
            pub async fn find_by_id(id: (#(#pk_types),*), db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<Self, modo::Error> {
                #find_body
            }
        },
        quote! {
            pub async fn delete_by_id(id: (#(#pk_types),*), db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<(), modo::Error> {
                #delete_body
            }
        },
    )
}

/// Generate `force_delete_by_id` method for soft-delete entities.
fn gen_force_delete_by_id(pk_fields: &[&ParsedField], mod_name: &Ident) -> TokenStream {
    if pk_fields.len() == 1 {
        let pk_ty = &pk_fields[0].ty;
        let is_string_pk = type_name_str(pk_ty).as_deref() == Some("String");

        if is_string_pk {
            quote! {
                /// Permanently delete a record by ID, bypassing soft-delete.
                pub async fn force_delete_by_id(id: &str, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<(), modo::Error> {
                    use modo_db::sea_orm::EntityTrait;
                    use modo_db::sea_orm::ModelTrait;
                    let model = #mod_name::Entity::find_by_id(id)
                        .one(db)
                        .await
                        .map_err(modo_db::db_err_to_error)?
                        .ok_or_else(|| modo::Error::from(modo::HttpError::NotFound))?;
                    model.delete(db).await.map_err(modo_db::db_err_to_error)?;
                    Ok(())
                }
            }
        } else {
            quote! {
                /// Permanently delete a record by ID, bypassing soft-delete.
                pub async fn force_delete_by_id(id: #pk_ty, db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<(), modo::Error> {
                    use modo_db::sea_orm::EntityTrait;
                    use modo_db::sea_orm::ModelTrait;
                    let model = #mod_name::Entity::find_by_id(id)
                        .one(db)
                        .await
                        .map_err(modo_db::db_err_to_error)?
                        .ok_or_else(|| modo::Error::from(modo::HttpError::NotFound))?;
                    model.delete(db).await.map_err(modo_db::db_err_to_error)?;
                    Ok(())
                }
            }
        }
    } else {
        let pk_types: Vec<&Type> = pk_fields.iter().map(|f| &f.ty).collect();
        quote! {
            /// Permanently delete a record by composite ID, bypassing soft-delete.
            pub async fn force_delete_by_id(id: (#(#pk_types),*), db: &impl modo_db::sea_orm::ConnectionTrait) -> Result<(), modo::Error> {
                use modo_db::sea_orm::EntityTrait;
                use modo_db::sea_orm::ModelTrait;
                let model = #mod_name::Entity::find_by_id(id)
                    .one(db)
                    .await
                    .map_err(modo_db::db_err_to_error)?
                    .ok_or_else(|| modo::Error::from(modo::HttpError::NotFound))?;
                model.delete(db).await.map_err(modo_db::db_err_to_error)?;
                Ok(())
            }
        }
    }
}
