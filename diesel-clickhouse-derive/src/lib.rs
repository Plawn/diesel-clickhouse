// Proc-macro crates: panics occur at compile-time, not runtime, so they're acceptable.
// Panics in proc-macros produce compile errors, which is the intended behavior.
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

//! Procedural macros for diesel-clickhouse.
//!
//! This crate provides:
//! - `table!` macro for defining table schemas
//! - `#[clickhouse_row]` attribute for row serialization (HTTP + Native backends)
//! - `#[derive(Queryable)]` for deserializing query results
//! - `#[derive(Insertable)]` for row serialization (INSERT operations)
//! - `#[derive(Selectable)]` for explicit column selection with compile-time verification
//!
//! # Usage
//!
//! ```rust,ignore
//! // Full table SELECT with compile-time verification
//! #[clickhouse_row(table_name = users)]
//! #[derive(Debug, Queryable, Selectable)]
//! struct User {
//!     id: u64,
//!     #[diesel_clickhouse(column_name = "user_name")]
//!     name: String,
//! }
//!
//! // Custom projection (JOINs, aggregations, etc.)
//! #[clickhouse_row]
//! #[derive(Debug, Queryable)]
//! struct UserWithPosts {
//!     user_id: u64,
//!     #[diesel_clickhouse(column_name = "post_count")]
//!     post_count: u64,
//! }
//!
//! // INSERT struct
//! #[derive(Debug, Insertable)]
//! #[diesel_clickhouse(table_name = events)]
//! struct NewEvent {
//!     id: u64,
//!     user_id: u64,
//! }
//! ```

use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, DeriveInput, Data, Fields, Ident, LitStr, Attribute, ItemStruct, Token};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;

mod table;

// =============================================================================
// Error Handling Helpers
// =============================================================================

/// Extract named fields from a DeriveInput, returning a compile error if not a struct with named fields.
fn extract_named_fields<'a>(input: &'a DeriveInput, derive_name: &str) -> Result<&'a Punctuated<syn::Field, syn::Token![,]>, TokenStream> {
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => Ok(&fields.named),
            Fields::Unnamed(_) => Err(syn::Error::new(
                data.fields.span(),
                format!("{} can only be derived for structs with named fields, not tuple structs", derive_name)
            ).to_compile_error().into()),
            Fields::Unit => Err(syn::Error::new(
                input.ident.span(),
                format!("{} can only be derived for structs with named fields, not unit structs", derive_name)
            ).to_compile_error().into()),
        },
        Data::Enum(e) => Err(syn::Error::new(
            e.enum_token.span(),
            format!("{} can only be derived for structs, not enums", derive_name)
        ).to_compile_error().into()),
        Data::Union(u) => Err(syn::Error::new(
            u.union_token.span(),
            format!("{} can only be derived for structs, not unions", derive_name)
        ).to_compile_error().into()),
    }
}

/// Get a required table_name attribute, returning a compile error if not present.
fn require_table_name_attribute(attrs: &[Attribute], derive_name: &str) -> Result<syn::Path, TokenStream> {
    get_table_name_attribute(attrs).ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("#[diesel_clickhouse(table_name = ...)] attribute is required for {}", derive_name)
        ).to_compile_error().into()
    })
}

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

// =============================================================================
// #[clickhouse_row(...)] Attribute Macro - Unified row serialization
// =============================================================================

/// Mark a struct as a ClickHouse row with optimized binary deserialization.
///
/// This attribute macro generates optimal deserialization code for both backends:
///
/// - **HTTP backend**: Adds `#[derive(clickhouse::Row)]` for RowBinary format (2-3x faster than JSON)
/// - **Native backend**: Generates `FromNativeBlock` for direct Block deserialization
///
/// # Basic Usage (Custom Projections, JOINs)
///
/// ```rust,ignore
/// #[clickhouse_row]
/// #[derive(Debug, Queryable)]
/// struct UserWithPosts {
///     user_id: u64,
///     #[diesel_clickhouse(column_name = "post_count")]
///     post_count: u64,
/// }
/// ```
///
/// # With Table Association (Full Table Queries)
///
/// ```rust,ignore
/// #[clickhouse_row(table_name = users)]
/// #[derive(Debug, Queryable, Selectable)]
/// struct User {
///     id: u64,
///     #[diesel_clickhouse(column_name = "user_name")]
///     name: String,
/// }
/// ```
///
/// # Column Renaming
///
/// Use `#[diesel_clickhouse(column_name = "...")]` on fields to map to a different database column:
///
/// ```rust,ignore
/// #[clickhouse_row]
/// #[derive(Debug, Queryable)]
/// struct User {
///     id: u64,
///     #[diesel_clickhouse(column_name = "user_name")]
///     name: String,
/// }
/// ```
///
/// # Generated Code
///
/// For HTTP backend (when `http` feature is enabled):
/// - Adds `#[derive(clickhouse::Row)]` with appropriate `#[serde(rename = "...")]` attributes
///
/// For Native backend (when `native` feature is enabled):
/// - Generates `impl FromNativeBlock for User { ... }`
/// - Generates `impl FromAnyBlock for User { ... }`
/// - Generates `impl ToNativeBlock for User { ... }`
#[proc_macro_attribute]
pub fn clickhouse_row(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let name = &input.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;
    let generics = &input.generics;

    // Parse the optional table_name attribute
    let table_path: Option<syn::Path> = if attr.is_empty() {
        None
    } else {
        match syn::parse::<ClickhouseRowAttr>(attr) {
            Ok(parsed) => parsed.table_name,
            Err(e) => return e.to_compile_error().into(),
        }
    };

    // Extract fields
    let fields = match &input.fields {
        Fields::Named(fields) => &fields.named,
        Fields::Unnamed(_) => {
            return syn::Error::new(
                input.fields.span(),
                "#[clickhouse_row] can only be used on structs with named fields, not tuple structs"
            ).to_compile_error().into();
        }
        Fields::Unit => {
            return syn::Error::new(
                input.ident.span(),
                "#[clickhouse_row] can only be used on structs with named fields, not unit structs"
            ).to_compile_error().into();
        }
    };

    // Parse field information
    let field_info = RowFieldInfo::from_fields(fields);

    // Generate code using shared helpers
    let struct_def = generate_struct_with_serde(attrs, vis, name, generics, &field_info);
    let native_impls = generate_native_block_impls(name, &field_info);

    // If table_name is provided, add a marker attribute for Selectable/Queryable derives to find
    // The #[clickhouse_row(table_name = X)] is consumed by this attribute macro, so we need to
    // re-emit it as #[diesel_clickhouse(table_name = X)] for the derive macros to read.
    let marker_attr = if let Some(ref table) = table_path {
        quote! { #[diesel_clickhouse(table_name = #table)] }
    } else {
        quote! {}
    };

    let expanded = quote! {
        #marker_attr
        #struct_def
        #native_impls
    };

    TokenStream::from(expanded)
}

/// Parser for #[clickhouse_row(table_name = path::to::table)]
struct ClickhouseRowAttr {
    table_name: Option<syn::Path>,
}

impl Parse for ClickhouseRowAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        if ident != "table_name" {
            return Err(syn::Error::new(
                ident.span(),
                "expected `table_name = path::to::table`"
            ));
        }
        input.parse::<Token![=]>()?;
        let table: syn::Path = input.parse()?;
        Ok(ClickhouseRowAttr { table_name: Some(table) })
    }
}

/// Derive `Queryable` for deserializing query results with compile-time type verification.
///
/// # For full table queries
///
/// Use `#[diesel_clickhouse(table_name = table_path)]` when your struct matches all columns:
///
/// ```rust,ignore
/// #[diesel_clickhouse(table_name = users)]
/// #[derive(Debug, Queryable, Selectable)]
/// struct User {
///     id: u64,
///     name: String,
///     email: Option<String>,
/// }
///
/// // Compile-time verified!
/// let users: Vec<User> = users::table.load(&conn).await?;
/// ```
///
/// # For custom SELECT projections (JOINs, aggregations)
///
/// Use `#[diesel_clickhouse]` without `table_name` - types are auto-deduced:
///
/// ```rust,ignore
/// #[diesel_clickhouse]
/// #[derive(Debug, Queryable)]
/// struct UserWithPosts {
///     user_id: u64,
///     #[diesel_clickhouse(column_name = "post_count")]
///     post_count: u64,
/// }
/// ```
#[proc_macro_derive(Queryable, attributes(diesel_clickhouse))]
pub fn derive_queryable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match extract_named_fields(&input, "Queryable") {
        Ok(fields) => fields,
        Err(err) => return err,
    };

    let field_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();
    let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();

    // Check for table_name attribute
    let table_path = get_table_name_attribute(&input.attrs);

    // Generate field indices for Queryable::build
    let field_indices: Vec<syn::Index> = (0..field_names.len())
        .map(syn::Index::from)
        .collect();

    // Generate Queryable impl based on whether table_name is provided.
    let queryable_impl_tokens = match table_path {
        Some(table_path) => {
            // Use table's AllColumnsSqlType for compile-time type verification.
            // This ensures the struct's fields match the table's column types.
            quote! {
                impl ::diesel_clickhouse::core::deserialize::Queryable<
                    <#table_path::table as ::diesel_clickhouse::core::query_source::Table>::AllColumnsSqlType
                > for #name {
                    type Row = (#(#field_types,)*);

                    fn build(row: Self::Row) -> ::diesel_clickhouse::core::result::QueryResult<Self> {
                        Ok(Self {
                            #(#field_names: row.#field_indices,)*
                        })
                    }
                }
            }
        }
        None => {
            // No table_name - deduce SQL types from Rust types via HasSqlType trait.
            // This works for custom projections (JOINs, aggregations) using common types
            // like String, u64, Vec<T>, etc. that implement HasSqlType.
            quote! {
                impl ::diesel_clickhouse::core::deserialize::Queryable<(
                    #(<#field_types as ::diesel_clickhouse::types::HasSqlType>::SqlType,)*
                )> for #name {
                    type Row = (#(#field_types,)*);

                    fn build(row: Self::Row) -> ::diesel_clickhouse::core::result::QueryResult<Self> {
                        Ok(Self {
                            #(#field_names: row.#field_indices,)*
                        })
                    }
                }
            }
        }
    };

    let expanded = quote! {
        #queryable_impl_tokens
    };

    TokenStream::from(expanded)
}

/// Derive `Insertable` for serializing rows for insertion.
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Insertable)]
/// #[diesel_clickhouse(table_name = events)]
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
/// Check if a type looks like a JSON type (JsonTyped<T> or serde_json::Value).
fn is_json_type(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(type_path) => {
            let path = &type_path.path;
            // Check last segment name
            if let Some(segment) = path.segments.last() {
                let name = segment.ident.to_string();
                // Match JsonTyped or Value (from serde_json)
                if name == "JsonTyped" || name == "Value" {
                    return true;
                }
            }
            // Check for full path like serde_json::Value
            let full_path: String = path.segments.iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            full_path.contains("serde_json") && full_path.contains("Value")
        }
        _ => false,
    }
}

#[proc_macro_derive(Insertable, attributes(diesel_clickhouse))]
pub fn derive_insertable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Get table name from attribute
    let table_path = match require_table_name_attribute(&input.attrs, "Insertable") {
        Ok(path) => path,
        Err(err) => return err,
    };

    let fields = match extract_named_fields(&input, "Insertable") {
        Ok(fields) => fields,
        Err(err) => return err,
    };

    // Check if any field is a JSON type
    let has_json_fields = fields.iter().any(|f| is_json_type(&f.ty));

    let column_names: Vec<String> = fields.iter()
        .map(|f| get_column_name(f).unwrap_or_else(|| {
            f.ident.as_ref()
                .expect("Named fields always have identifiers")
                .to_string()
        }))
        .collect();

    let field_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();
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
            const REQUIRES_SQL_INSERT: bool = #has_json_fields;

            fn column_names() -> &'static [&'static str] {
                static COLUMNS: [&str; #column_count] = [#(#column_names_array),*];
                &COLUMNS
            }

            fn write_value<DB: diesel_clickhouse::backend::Backend>(&self, pass: &mut diesel_clickhouse::query_builder::AstPass<'_, '_, DB>) -> diesel_clickhouse::result::QueryResult<()> {
                #(#write_fields)*
                Ok(())
            }
        }

        // Generate ToSqlValues for async insert support
        impl #impl_generics diesel_clickhouse::serialize::ToSqlValues for #name #ty_generics #where_clause {
            fn column_names() -> Vec<&'static str> {
                vec![#(#column_names_array),*]
            }

            fn to_sql_values(&self) -> Vec<String> {
                vec![
                    #(
                        diesel_clickhouse::serialize::ToSqlLiteral::to_sql_literal(&self.#field_names).into_owned(),
                    )*
                ]
            }
        }
    };

    TokenStream::from(expanded)
}

/// Derive `Selectable` for explicit column selection with compile-time verification.
///
/// This derive generates an `as_select()` method that returns a tuple of column references,
/// and verifies at compile-time that all columns exist in the specified table.
///
/// # Example
///
/// ```rust,ignore
/// #[diesel_clickhouse(table_name = users)]
/// #[derive(Debug, Queryable, Selectable)]
/// struct User {
///     id: u64,
///     #[diesel_clickhouse(column_name = "user_name")]
///     name: String,
/// }
///
/// // Use with type-safe column selection
/// let selection = User::as_select();
/// ```
#[proc_macro_derive(Selectable, attributes(diesel_clickhouse))]
pub fn derive_selectable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let table_path = match require_table_name_attribute(&input.attrs, "Selectable") {
        Ok(path) => path,
        Err(err) => return err,
    };

    let fields = match extract_named_fields(&input, "Selectable") {
        Ok(fields) => fields,
        Err(err) => return err,
    };

    let column_names: Vec<String> = fields.iter()
        .map(|f| get_column_name(f).unwrap_or_else(|| {
            f.ident.as_ref()
                .expect("Named fields always have identifiers")
                .to_string()
        }))
        .collect();

    let column_idents: Vec<Ident> = column_names.iter()
        .map(|name| format_ident!("{}", name))
        .collect();

    let column_refs: Vec<_> = column_idents.iter()
        .map(|col_ident| quote! { #table_path::#col_ident })
        .collect();

    let expanded = quote! {
        // Compile-time verification that all column names exist in the table
        const _: () = {
            #[allow(unused)]
            fn _verify_column_names_for_selectable() {
                #(
                    let _ = #table_path::#column_idents;
                )*
            }
        };

        impl #impl_generics #name #ty_generics #where_clause {
            /// Get the selection expression for this type.
            ///
            /// Returns a tuple of column references that can be used in `.select()`.
            pub fn as_select() -> (#(#column_refs,)*) {
                (#(#column_refs,)*)
            }

            /// Alias for `as_select()` for backward compatibility.
            #[deprecated(note = "Use as_select() instead")]
            pub fn selection() -> (#(#column_refs,)*) {
                Self::as_select()
            }
        }
    };

    TokenStream::from(expanded)
}

// =============================================================================
// Helper functions
// =============================================================================

/// Parse column name from field attributes.
/// Supports: `#[diesel_clickhouse(column_name = "...")]`
fn get_column_name(field: &syn::Field) -> Option<String> {
    for attr in &field.attrs {
        if attr.path().is_ident("diesel_clickhouse") {
            // Parse #[diesel_clickhouse(column_name = "...")]
            let result = attr.parse_args_with(|input: syn::parse::ParseStream| {
                let ident: Ident = input.parse()?;
                if ident != "column_name" {
                    return Err(input.error("expected 'column_name'"));
                }
                input.parse::<syn::Token![=]>()?;
                let lit: LitStr = input.parse()?;
                Ok(lit.value())
            });
            if let Ok(name) = result {
                return Some(name);
            }
        }
    }
    None
}

/// Parse table_name from struct attributes.
/// Supports: `#[diesel_clickhouse(table_name = path::to::table)]`
fn get_table_name_attribute(attrs: &[Attribute]) -> Option<syn::Path> {
    for attr in attrs {
        if attr.path().is_ident("diesel_clickhouse") {
            // Parse #[diesel_clickhouse(table_name = path::to::table)]
            let result = attr.parse_args_with(|input: syn::parse::ParseStream| {
                let ident: Ident = input.parse()?;
                if ident != "table_name" {
                    return Err(input.error("expected 'table_name'"));
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

// =============================================================================
// Shared Code Generation Helpers for #[diesel_clickhouse] attribute macro
// =============================================================================

use proc_macro2::TokenStream as TokenStream2;

/// Parsed field information for code generation.
struct RowFieldInfo<'a> {
    /// Field identifiers (e.g., `id`, `name`)
    names: Vec<&'a Option<Ident>>,
    /// Field types (e.g., `u64`, `String`)
    types: Vec<&'a syn::Type>,
    /// Field visibility (e.g., `pub`)
    vis: Vec<&'a syn::Visibility>,
    /// Non-column_name attributes to preserve
    attrs: Vec<Vec<&'a Attribute>>,
    /// Database column names (from field name or #[column_name])
    column_names: Vec<String>,
    /// Serde rename attributes for fields with different column names
    serde_renames: Vec<TokenStream2>,
    /// Variable names for column vectors in ToNativeBlock
    col_var_names: Vec<Ident>,
}

impl<'a> RowFieldInfo<'a> {
    /// Parse field information from a struct's named fields.
    fn from_fields(fields: &'a syn::punctuated::Punctuated<syn::Field, syn::Token![,]>) -> Self {
        let names: Vec<_> = fields.iter().map(|f| &f.ident).collect();
        let types: Vec<_> = fields.iter().map(|f| &f.ty).collect();
        let vis: Vec<_> = fields.iter().map(|f| &f.vis).collect();
        // Filter out #[diesel_clickhouse(...)] attributes (they contain column_name which we handle separately)
        let attrs: Vec<Vec<_>> = fields.iter().map(|f| {
            f.attrs.iter().filter(|a| !a.path().is_ident("diesel_clickhouse")).collect()
        }).collect();

        let column_names: Vec<String> = fields.iter()
            .map(|f| get_column_name(f).unwrap_or_else(|| {
                f.ident.as_ref()
                    .expect("Named fields always have identifiers")
                    .to_string()
            }))
            .collect();

        let serde_renames: Vec<_> = column_names.iter().zip(names.iter()).map(|(col, field)| {
            let field_str = field.as_ref()
                .expect("Named fields always have identifiers")
                .to_string();
            if col != &field_str {
                quote! { #[serde(rename = #col)] }
            } else {
                quote! {}
            }
        }).collect();

        let col_var_names: Vec<_> = names.iter()
            .map(|f| format_ident!("col_{}", f.as_ref().expect("Named fields always have identifiers")))
            .collect();

        Self { names, types, vis, attrs, column_names, serde_renames, col_var_names }
    }
}

/// Generate struct definition with serde and clickhouse::Row derives.
fn generate_struct_with_serde(
    attrs: &[Attribute],
    vis: &syn::Visibility,
    name: &Ident,
    generics: &syn::Generics,
    field_info: &RowFieldInfo<'_>,
) -> TokenStream2 {
    let where_clause = &generics.where_clause;
    let field_names = &field_info.names;
    let field_types = &field_info.types;
    let field_attrs = &field_info.attrs;
    let field_vis = &field_info.vis;
    let serde_renames = &field_info.serde_renames;

    quote! {
        #(#attrs)*
        #[cfg_attr(feature = "http", derive(::diesel_clickhouse::clickhouse::Row))]
        #[cfg_attr(feature = "http", derive(::serde::Serialize, ::serde::Deserialize))]
        #vis struct #name #generics #where_clause {
            #(
                #(#field_attrs)*
                #serde_renames
                #field_vis #field_names: #field_types,
            )*
        }
    }
}

/// Generate FromNativeBlock and FromAnyBlock implementations (for reading).
fn generate_from_native_impls(name: &Ident, field_info: &RowFieldInfo<'_>) -> TokenStream2 {
    let field_names = &field_info.names;
    let field_types = &field_info.types;
    let column_names = &field_info.column_names;

    quote! {
        // Generate FromNativeBlock for Native backend
        #[cfg(feature = "native")]
        impl ::diesel_clickhouse::native::FromNativeBlock for #name {
            fn from_block_row(
                block: &::diesel_clickhouse::native::ComplexBlock,
                row_idx: usize,
            ) -> ::diesel_clickhouse::result::QueryResult<Self> {
                Ok(Self {
                    #(
                        #field_names: ::diesel_clickhouse::native::BlockValue::get_value(block, row_idx, #column_names)?,
                    )*
                })
            }
        }

        // Generate FromAnyBlock for Native backend streaming
        #[cfg(feature = "native")]
        impl ::diesel_clickhouse::native::FromAnyBlock for #name {
            fn from_any_block<K: ::diesel_clickhouse::native::types::ColumnType>(
                block: &::diesel_clickhouse::native::NativeBlock<K>,
                row_idx: usize,
            ) -> ::diesel_clickhouse::result::QueryResult<Self> {
                Ok(Self {
                    #(
                        #field_names: <#field_types as ::diesel_clickhouse::native::BlockValue<K>>::get_value(block, row_idx, #column_names)?,
                    )*
                })
            }
        }
    }
}

/// Generate ToNativeBlock implementation (for writing/INSERT).
fn generate_to_native_impl(name: &Ident, field_info: &RowFieldInfo<'_>) -> TokenStream2 {
    let field_names = &field_info.names;
    let field_types = &field_info.types;
    let column_names = &field_info.column_names;
    let col_var_names = &field_info.col_var_names;

    quote! {
        // Generate ToNativeBlock for Native backend (optimized INSERT)
        #[cfg(feature = "native")]
        impl ::diesel_clickhouse::native::ToNativeBlock for #name {
            fn column_names() -> &'static [&'static str] {
                &[#(#column_names),*]
            }

            fn rows_to_block(rows: &[Self]) -> ::diesel_clickhouse::result::QueryResult<::diesel_clickhouse::native::NativeBlock> {
                // Pre-allocate columns with known capacity to avoid reallocations
                let capacity = rows.len();
                #(
                    let mut #col_var_names: <#field_types as ::diesel_clickhouse::native::IntoBlockColumn>::ColumnData =
                        <#field_types as ::diesel_clickhouse::native::IntoBlockColumn>::new_column_with_capacity(capacity);
                )*

                // Fill columns - allows LLVM to vectorize each column independently
                for row in rows {
                    #(
                        <#field_types as ::diesel_clickhouse::native::IntoBlockColumn>::push_to_column(&row.#field_names, &mut #col_var_names);
                    )*
                }

                let block = ::diesel_clickhouse::native::NativeBlock::new();
                #(
                    let block = <#field_types as ::diesel_clickhouse::native::IntoBlockColumn>::add_column_to_block(block, #column_names, #col_var_names);
                )*

                Ok(block)
            }

            fn rows_into_block(rows: Vec<Self>) -> ::diesel_clickhouse::result::QueryResult<::diesel_clickhouse::native::NativeBlock> {
                let capacity = rows.len();
                #(
                    let mut #col_var_names: <#field_types as ::diesel_clickhouse::native::IntoBlockColumn>::ColumnData =
                        <#field_types as ::diesel_clickhouse::native::IntoBlockColumn>::new_column_with_capacity(capacity);
                )*

                for row in rows {
                    #(
                        <#field_types as ::diesel_clickhouse::native::IntoBlockColumnOwned>::push_to_column_owned(row.#field_names, &mut #col_var_names);
                    )*
                }

                let block = ::diesel_clickhouse::native::NativeBlock::new();
                #(
                    let block = <#field_types as ::diesel_clickhouse::native::IntoBlockColumn>::add_column_to_block(block, #column_names, #col_var_names);
                )*

                Ok(block)
            }
        }
    }
}

/// Generate FromNativeBlock, FromAnyBlock, and ToNativeBlock implementations.
fn generate_native_block_impls(name: &Ident, field_info: &RowFieldInfo<'_>) -> TokenStream2 {
    let from_impls = generate_from_native_impls(name, field_info);
    let to_impl = generate_to_native_impl(name, field_info);

    quote! {
        #from_impls
        #to_impl
    }
}
