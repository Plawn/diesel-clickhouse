//! Procedural macros for diesel-clickhouse.
//!
//! This crate provides:
//! - `table!` macro for defining table schemas
//! - `#[derive(Queryable)]` for row deserialization
//! - `#[derive(Insertable)]` for row serialization
//! - `#[derive(Selectable)]` for explicit column selection

use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, DeriveInput, Data, Fields, Ident, LitStr, Attribute};

mod table;

/// Define a ClickHouse table schema.
///
/// # Example
///
/// ```rust,ignore
/// diesel_clickhouse::table! {
///     events (id, timestamp) {
///         id -> UInt64,
///         user_id -> UInt32,
///         event_type -> LowCardinality<CHString>,
///         timestamp -> DateTime64<3>,
///         tags -> Array<CHString>,
///     }
/// }
/// ```
///
/// This generates:
/// - A module with the table name
/// - Column types for each column
/// - Implementations of `Table`, `Column`, and `QuerySource` traits
#[proc_macro]
pub fn table(input: TokenStream) -> TokenStream {
    table::table_impl(input)
}

/// Derive `Queryable` for deserializing query results.
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Queryable)]
/// struct User {
///     id: u64,
///     name: String,
///     email: Option<String>,
/// }
/// ```
#[proc_macro_derive(Queryable, attributes(diesel_clickhouse, column_name))]
pub fn derive_queryable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("Queryable can only be derived for structs with named fields"),
        },
        _ => panic!("Queryable can only be derived for structs"),
    };

    let field_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();
    let field_names_str: Vec<_> = fields.iter()
        .map(|f| get_column_name(f).unwrap_or_else(|| f.ident.as_ref().unwrap().to_string()))
        .collect();

    let expanded = quote! {
        impl #impl_generics diesel_clickhouse::deserialize::FromRow for #name #ty_generics #where_clause {
            fn from_row(row: &dyn diesel_clickhouse::result::Row) -> diesel_clickhouse::result::QueryResult<Self> {
                let fields = diesel_clickhouse::deserialize::RowFields::new(row);
                Ok(Self {
                    #(#field_names: fields.get(#field_names_str)?,)*
                })
            }
        }
    };

    TokenStream::from(expanded)
}

/// Derive `Insertable` for serializing rows for insertion.
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Insertable)]
/// #[diesel_clickhouse(table = events)]
/// struct NewEvent {
///     id: u64,
///     user_id: u32,
///     event_type: String,
/// }
///
/// // Usage:
/// let query = insert_into(events::table).values(&new_event);
/// // Generates: INSERT INTO events (id, user_id, event_type) VALUES (1, 42, 'hello')
/// ```
#[proc_macro_derive(Insertable, attributes(diesel_clickhouse, column_name))]
pub fn derive_insertable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Get table name from attribute
    let table_path = get_table_attribute(&input.attrs)
        .expect("#[diesel_clickhouse(table = ...)] attribute is required for Insertable");

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("Insertable can only be derived for structs with named fields"),
        },
        _ => panic!("Insertable can only be derived for structs"),
    };

    let column_names: Vec<String> = fields.iter()
        .map(|f| get_column_name(f).unwrap_or_else(|| f.ident.as_ref().unwrap().to_string()))
        .collect();

    let field_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();
    let _field_count = field_names.len();

    let column_count = column_names.len();
    let column_names_array = &column_names;

    // Generate the write_value implementation that outputs SQL literals
    let write_fields = field_names.iter().enumerate().map(|(i, field_name)| {
        if i > 0 {
            quote! {
                pass.push_sql(", ");
                diesel_clickhouse::serialize::write_sql_value(&self.#field_name, pass);
            }
        } else {
            quote! {
                diesel_clickhouse::serialize::write_sql_value(&self.#field_name, pass);
            }
        }
    });

    let expanded = quote! {
        impl #impl_generics diesel_clickhouse::serialize::ToRow for #name #ty_generics #where_clause {
            fn column_names() -> &'static [&'static str] {
                static COLUMNS: [&str; #column_count] = [#(#column_names_array),*];
                &COLUMNS
            }

            fn to_row_bytes(&self, out: &mut Vec<u8>) -> diesel_clickhouse::result::QueryResult<()> {
                #(
                    diesel_clickhouse::serialize::write_sql_bytes(&self.#field_names, out);
                )*
                Ok(())
            }
        }

        impl #impl_generics diesel_clickhouse::query_builder::Insertable<#table_path::table> for #name #ty_generics #where_clause {
            fn column_names() -> &'static [&'static str] {
                static COLUMNS: [&str; #column_count] = [#(#column_names_array),*];
                &COLUMNS
            }

            fn write_value<DB: diesel_clickhouse::backend::Backend>(&self, pass: &mut diesel_clickhouse::query_builder::AstPass<'_, '_, DB>) -> diesel_clickhouse::result::QueryResult<()> {
                #(#write_fields)*
                Ok(())
            }
        }
    };

    TokenStream::from(expanded)
}

/// Derive `Selectable` for explicit column selection.
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Selectable)]
/// #[diesel_clickhouse(table = users)]
/// struct UserSummary {
///     id: u64,
///     name: String,
/// }
/// ```
#[proc_macro_derive(Selectable, attributes(diesel_clickhouse, column_name))]
pub fn derive_selectable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let table_path = get_table_attribute(&input.attrs)
        .expect("#[diesel_clickhouse(table = ...)] attribute is required for Selectable");

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("Selectable can only be derived for structs with named fields"),
        },
        _ => panic!("Selectable can only be derived for structs"),
    };

    let column_refs: Vec<_> = fields.iter().map(|f| {
        let col_name = get_column_name(f)
            .unwrap_or_else(|| f.ident.as_ref().unwrap().to_string());
        let col_ident = format_ident!("{}", col_name);
        quote! { #table_path::#col_ident }
    }).collect();

    let expanded = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Get the selection expression for this type.
            pub fn selection() -> (#(#column_refs,)*) {
                (#(#column_refs,)*)
            }
        }
    };

    TokenStream::from(expanded)
}

// =============================================================================
// Helper functions
// =============================================================================

fn get_column_name(field: &syn::Field) -> Option<String> {
    for attr in &field.attrs {
        if attr.path().is_ident("column_name") {
            if let Ok(lit) = attr.parse_args::<LitStr>() {
                return Some(lit.value());
            }
        }
        if attr.path().is_ident("diesel_clickhouse") {
            // Parse #[diesel_clickhouse(column_name = "...")]
            // Simplified parsing
        }
    }
    None
}

fn get_table_attribute(attrs: &[Attribute]) -> Option<syn::Path> {
    for attr in attrs {
        if attr.path().is_ident("diesel_clickhouse") {
            // Parse #[diesel_clickhouse(table = path::to::table)]
            let result = attr.parse_args_with(|input: syn::parse::ParseStream| {
                let ident: Ident = input.parse()?;
                if ident != "table" {
                    return Err(input.error("expected 'table'"));
                }
                input.parse::<syn::Token![=]>()?;
                let path: syn::Path = input.parse()?;
                Ok(path)
            });
            if let Ok(path) = result {
                return Some(path);
            }
        }
    }
    None
}
