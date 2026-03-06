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

// ---------------------------------------------------------------------------
// Parsing types
// ---------------------------------------------------------------------------

struct EntityArgs {
    table_name: String,
}

impl syn::parse::Parse for EntityArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut table_name = None;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match ident.to_string().as_str() {
                "table" => {
                    let val: LitStr = input.parse()?;
                    table_name = Some(val.value());
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
        Ok(EntityArgs { table_name })
    }
}

struct StructAttrs {
    timestamps: bool,
    soft_delete: bool,
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
                Err(meta.error("expected `timestamps`, `soft_delete`, or `index(...)`"))
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
// Code generation
// ---------------------------------------------------------------------------

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args: EntityArgs = parse2(attr)?;
    let mut input: ItemStruct = parse2(item)?;

    let struct_name = input.ident.clone();
    let mod_name = format_ident!("{}", to_snake_case(&struct_name.to_string()));
    let table_name = &args.table_name;

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
            let val_str = match lit {
                Lit::Int(i) => i.to_string(),
                Lit::Float(f) => f.to_string(),
                Lit::Str(s) => s.value(),
                Lit::Bool(b) => b.value.to_string(),
                _ => quote!(#lit).to_string(),
            };
            sea_orm_attrs.push(quote! { default_value = #val_str });
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

        let target = if f.attrs.has_many {
            f.attrs.via.as_ref().map_or_else(
                || {
                    to_pascal_case(&f.name.to_string())
                        .trim_end_matches('s')
                        .to_string()
                },
                |_via| {
                    to_pascal_case(&f.name.to_string())
                        .trim_end_matches('s')
                        .to_string()
                },
            )
        } else {
            to_pascal_case(&f.name.to_string())
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

    let auto_id_stmts: Vec<_> = parsed_fields
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
                    if modo_db::sea_orm::ActiveValue::is_not_set(&this.#name) {
                        this.#name = modo_db::sea_orm::ActiveValue::Set(#gen_call);
                    }
                }
            })
        })
        .collect();

    let needs_before_save = struct_attrs.timestamps || !auto_id_stmts.is_empty();

    let active_model_behavior = if needs_before_save {
        let timestamp_stmts = if struct_attrs.timestamps {
            quote! {
                let now = modo_db::chrono::Utc::now();
                if insert {
                    this.created_at = modo_db::sea_orm::ActiveValue::Set(now);
                }
                this.updated_at = modo_db::sea_orm::ActiveValue::Set(now);
            }
        } else {
            quote! {}
        };

        let auto_id_block = if !auto_id_stmts.is_empty() {
            quote! {
                if insert {
                    #(#auto_id_stmts)*
                }
            }
        } else {
            quote! {}
        };

        quote! {
            #[async_trait::async_trait]
            impl ActiveModelBehavior for ActiveModel {
                async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
                where
                    C: ConnectionTrait,
                {
                    let mut this = self;
                    #auto_id_block
                    #timestamp_stmts
                    Ok(this)
                }
            }
        }
    } else {
        quote! {
            impl ActiveModelBehavior for ActiveModel {}
        }
    };

    let soft_delete_helpers = if struct_attrs.soft_delete {
        quote! {
            pub fn find_active() -> modo_db::sea_orm::Select<Entity> {
                use modo_db::sea_orm::EntityTrait;
                use modo_db::sea_orm::QueryFilter;
                use modo_db::sea_orm::ColumnTrait;
                Entity::find().filter(Column::DeletedAt.is_null())
            }

            pub async fn soft_delete<C: modo_db::sea_orm::ConnectionTrait>(
                mut model: ActiveModel,
                db: &C,
            ) -> std::result::Result<Model, modo_db::sea_orm::DbErr> {
                use modo_db::sea_orm::ActiveModelTrait;
                model.deleted_at = modo_db::sea_orm::ActiveValue::Set(Some(modo_db::chrono::Utc::now()));
                model.update(db).await
            }

            pub async fn force_delete<C: modo_db::sea_orm::ConnectionTrait>(
                model: Model,
                db: &C,
            ) -> std::result::Result<modo_db::sea_orm::DeleteResult, modo_db::sea_orm::DbErr> {
                use modo_db::sea_orm::ModelTrait;
                model.delete(db).await
            }
        }
    } else {
        quote! {}
    };

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

    let extra_sql_tokens = if extra_sql_stmts.is_empty() {
        quote! { &[] }
    } else {
        quote! { &[#(#extra_sql_stmts),*] }
    };

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

    Ok(quote! {
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

            #soft_delete_helpers
        }

        modo_db::inventory::submit! {
            modo_db::EntityRegistration {
                table_name: #table_name,
                create_table: |backend| {
                    let schema = modo_db::sea_orm::Schema::new(backend);
                    schema.create_table_from_entity(#mod_name::Entity)
                },
                is_framework: false,
                extra_sql: #extra_sql_tokens,
            }
        }
    })
}
