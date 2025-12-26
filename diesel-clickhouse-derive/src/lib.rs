// Proc-macro crates: panics occur at compile-time, not runtime, so they're acceptable.
// Panics in proc-macros produce compile errors, which is the intended behavior.
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

//! Procedural macros for diesel-clickhouse.
//!
//! This crate provides:
//! - `table!` macro for defining table schemas
//! - `#[row]` attribute for optimized binary row deserialization (recommended)
//! - `#[derive(Row)]` for serde-based row deserialization (deprecated, use `#[row]` instead)
//! - `#[derive(Queryable)]` for row deserialization
//! - `#[derive(Insertable)]` for row serialization
//! - `#[derive(Selectable)]` for explicit column selection

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

/// Get a required table attribute, returning a compile error if not present.
fn require_table_attribute(attrs: &[Attribute], derive_name: &str) -> Result<syn::Path, TokenStream> {
    get_table_attribute(attrs).ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("#[diesel_clickhouse(table = ...)] attribute is required for {}", derive_name)
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
// #[row] Attribute Macro - Unified optimized row deserialization
// =============================================================================

/// Mark a struct as a ClickHouse row with optimized binary deserialization.
///
/// This attribute macro generates optimal deserialization code for both backends:
///
/// - **HTTP backend**: Adds `#[derive(clickhouse::Row)]` for RowBinary format (2-3x faster than JSON)
/// - **Native backend**: Generates `FromNativeBlock` for direct Block deserialization
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::row;
///
/// #[row]
/// #[derive(Debug, Clone)]
/// struct User {
///     id: u64,
///     name: String,
///     email: Option<String>,
/// }
///
/// // Works optimally with both backends!
/// let users: Vec<User> = conn.load(users::table.filter(users::active.eq(true))).await?;
/// ```
///
/// # Column Renaming
///
/// Use `#[column_name("...")]` to map a field to a different database column:
///
/// ```rust,ignore
/// #[row]
/// struct User {
///     id: u64,
///     #[column_name("user_name")]
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
///
/// # Why an attribute macro instead of `#[derive(...)]`?
///
/// You might wonder why `#[row]` is an attribute macro rather than a derive macro like
/// `#[derive(Row)]`. This is a deliberate design choice due to Rust's macro limitations:
///
/// **Derive macros cannot modify the struct definition.** They can only *add* new
/// implementations after the struct is defined. However, `#[row]` needs to:
///
/// 1. **Add `#[derive(clickhouse::Row)]`** - The `clickhouse` crate's `Row` derive is
///    required for efficient RowBinary deserialization. A derive macro cannot add
///    other derive macros to a struct.
///
/// 2. **Add `#[derive(serde::Deserialize)]`** - Required for JSON fallback and the
///    `clickhouse::Row` derive itself.
///
/// 3. **Transform `#[column_name("x")]` into `#[serde(rename = "x")]`** - The serde
///    rename attribute must be present on the struct fields *before* serde's derive
///    runs. A derive macro runs too late to inject these attributes.
///
/// **Alternative: Manual derives**
///
/// If you prefer explicit derives, you can skip `#[row]` and manually add everything:
///
/// ```rust,ignore
/// #[derive(Debug, Clone)]
/// #[derive(serde::Deserialize)]
/// #[cfg_attr(feature = "http", derive(clickhouse::Row))]
/// struct User {
///     id: u64,
///     #[serde(rename = "user_name")]
///     name: String,
/// }
///
/// // Then implement FromNativeBlock manually or use #[derive(Row)] for that part
/// ```
///
/// The `#[row]` attribute provides a simpler, single-annotation solution that handles
/// all of this automatically.
#[proc_macro_attribute]
pub fn row(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let name = &input.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;
    let generics = &input.generics;

    // Extract fields
    let fields = match &input.fields {
        Fields::Named(fields) => &fields.named,
        Fields::Unnamed(_) => {
            return syn::Error::new(
                input.fields.span(),
                "#[row] can only be used on structs with named fields, not tuple structs"
            ).to_compile_error().into();
        }
        Fields::Unit => {
            return syn::Error::new(
                input.ident.span(),
                "#[row] can only be used on structs with named fields, not unit structs"
            ).to_compile_error().into();
        }
    };

    // Parse field information
    let field_info = RowFieldInfo::from_fields(fields);

    // Generate code using shared helpers
    let struct_def = generate_struct_with_serde(attrs, vis, name, generics, &field_info);
    let native_impls = generate_native_block_impls(name, &field_info);

    let expanded = quote! {
        #struct_def
        #native_impls
    };

    TokenStream::from(expanded)
}

/// Attribute macro for type-safe row deserialization with compile-time verification.
///
/// This macro extends `#[row]` to add compile-time type checking that ensures
/// the struct matches the table's column types.
///
/// # Usage
///
/// ```rust,ignore
/// use diesel_clickhouse::typed_row;
///
/// diesel_clickhouse::table! {
///     users (id) {
///         id -> UInt64,
///         name -> CHString,
///         active -> Bool,
///     }
/// }
///
/// #[typed_row(table = users)]
/// #[derive(Debug)]
/// struct User {
///     id: u64,
///     name: String,
///     active: bool,
/// }
///
/// // Now this produces a compile-time error if User doesn't match users::table:
/// let users: Vec<User> = conn.load(users::table).await?;
/// ```
///
/// # Generated Code
///
/// This macro generates everything that `#[row]` generates, plus:
/// - `impl Queryable<table::AllColumnsSqlType> for User`
#[proc_macro_attribute]
pub fn typed_row(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let name = &input.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;
    let generics = &input.generics;

    // Parse the table attribute: table = path::to::table
    let table_path: syn::Path = match syn::parse::<TypedRowAttr>(attr) {
        Ok(parsed) => parsed.table,
        Err(e) => return e.to_compile_error().into(),
    };

    // Extract fields
    let fields = match &input.fields {
        Fields::Named(fields) => &fields.named,
        Fields::Unnamed(_) => {
            return syn::Error::new(
                input.fields.span(),
                "#[typed_row] can only be used on structs with named fields, not tuple structs"
            ).to_compile_error().into();
        }
        Fields::Unit => {
            return syn::Error::new(
                input.ident.span(),
                "#[typed_row] can only be used on structs with named fields, not unit structs"
            ).to_compile_error().into();
        }
    };

    // Parse field information using shared helper
    let field_info = RowFieldInfo::from_fields(fields);

    // Generate shared code using helpers
    let struct_def = generate_struct_with_serde(attrs, vis, name, generics, &field_info);
    let native_impls = generate_native_block_impls(name, &field_info);

    // Generate typed_row-specific code: column verification and Queryable impl
    let typed_row_extras = generate_typed_row_extras(name, &table_path, &field_info);

    let expanded = quote! {
        #struct_def
        #typed_row_extras
        #native_impls
    };

    TokenStream::from(expanded)
}

/// Parser for #[typed_row(table = path::to::table)]
struct TypedRowAttr {
    table: syn::Path,
}

impl Parse for TypedRowAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        if ident != "table" {
            return Err(syn::Error::new(
                ident.span(),
                "expected `table = path::to::table`"
            ));
        }
        input.parse::<Token![=]>()?;
        let table: syn::Path = input.parse()?;
        Ok(TypedRowAttr { table })
    }
}

/// Derive `Row` for unified row deserialization.
///
/// This derive macro generates implementations that work with both
/// HTTP and Native backends:
///
/// - Generates `serde::Serialize` and `serde::Deserialize`
/// - Generates `FromNativeBlock` for Native backend direct deserialization
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::Row;
///
/// #[derive(Debug, Row)]
/// struct User {
///     id: u64,
///     name: String,
///     email: Option<String>,
/// }
///
/// // Works with both backends
/// let users: Vec<User> = conn.load(users::table.filter(users::active.eq(true))).await?;
/// ```
///
/// # For Maximum HTTP Performance
///
/// Add `clickhouse::Row` derive for RowBinary format (2-3x faster):
///
/// ```rust,ignore
/// use diesel_clickhouse::{Row, clickhouse};
///
/// #[derive(Debug, Row, clickhouse::Row)]
/// struct User {
///     id: u64,
///     name: String,
/// }
///
/// // Now you can use load() for best performance
/// let users: Vec<User> = conn.load(query).await?;
/// ```
///
/// # Attributes
///
/// - `#[column_name = "..."]` - Rename a field for the database column
/// - `#[serde(rename = "...")]` - Also supported for serde compatibility
///
/// # Generated Code
///
/// - `impl serde::Serialize for User`
/// - `impl serde::Deserialize for User`
/// - `impl FromNativeBlock for User` (Native backend, direct Block access)
#[proc_macro_derive(Row, attributes(column_name, serde))]
pub fn derive_row(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (_impl_generics, _ty_generics, where_clause) = generics.split_for_impl();

    let fields = match extract_named_fields(&input, "Row") {
        Ok(fields) => fields,
        Err(err) => return err,
    };

    // Collect field info
    let field_count = fields.len();
    let field_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();
    let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();
    let column_names: Vec<String> = fields.iter()
        .map(|f| get_column_name(f).unwrap_or_else(|| {
            f.ident.as_ref()
                .expect("Named fields always have identifiers")
                .to_string()
        }))
        .collect();

    // Generate serde rename attributes for Deserialize
    let serde_field_attrs: Vec<_> = column_names.iter().zip(field_names.iter()).map(|(col, field)| {
        let field_str = field.as_ref()
            .expect("Named fields always have identifiers")
            .to_string();
        if col != &field_str {
            quote! { #[serde(rename = #col)] }
        } else {
            quote! {}
        }
    }).collect();

    let expanded = quote! {
        // Implement serde::Deserialize by delegation
        impl<'de> ::serde::Deserialize<'de> for #name #where_clause
        where
            #(#field_types: ::serde::Deserialize<'de>,)*
        {
            fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                #[derive(::serde::Deserialize)]
                struct __DieselClickhouseRowHelper {
                    #(
                        #serde_field_attrs
                        #field_names: #field_types,
                    )*
                }

                let helper = __DieselClickhouseRowHelper::deserialize(deserializer)?;
                ::std::result::Result::Ok(Self {
                    #(#field_names: helper.#field_names,)*
                })
            }
        }

        // Implement serde::Serialize
        impl ::serde::Serialize for #name #where_clause
        where
            #(#field_types: ::serde::Serialize,)*
        {
            fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                use ::serde::ser::SerializeStruct;
                let mut state = serializer.serialize_struct(stringify!(#name), #field_count)?;
                #(
                    state.serialize_field(#column_names, &self.#field_names)?;
                )*
                state.end()
            }
        }

        // Implement FromNativeBlock for direct Native backend deserialization
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

        // Implement FromAnyBlock for Native backend streaming
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
    };

    TokenStream::from(expanded)
}

/// Derive `Queryable` for deserializing query results with compile-time type verification.
///
/// # For full table queries
///
/// Use `#[diesel_clickhouse(table = table_name)]` when your struct matches all columns:
///
/// ```rust,ignore
/// #[derive(Queryable)]
/// #[diesel_clickhouse(table = users)]
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
/// # For custom SELECT projections
///
/// Use `#[diesel_clickhouse(select = (Type1, Type2, ...))]` for custom column selections:
///
/// ```rust,ignore
/// use diesel_clickhouse::types::*;
///
/// #[derive(Queryable)]
/// #[diesel_clickhouse(select = (CHString, CHString))]
/// struct UserSummary {
///     name: String,
///     email: String,
/// }
///
/// // Compile-time verified!
/// let summaries: Vec<UserSummary> = users::table
///     .select((users::name, users::email))
///     .load(&conn)
///     .await?;
/// ```
#[proc_macro_derive(Queryable, attributes(diesel_clickhouse, column_name))]
pub fn derive_queryable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match extract_named_fields(&input, "Queryable") {
        Ok(fields) => fields,
        Err(err) => return err,
    };

    let field_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();
    let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();

    // Check for table or select attribute
    let queryable_impl = get_queryable_attribute(&input.attrs);

    // Generate field indices for Queryable::build
    let field_indices: Vec<syn::Index> = (0..field_names.len())
        .map(syn::Index::from)
        .collect();

    let queryable_impl_tokens = match queryable_impl {
        Some(QueryableAttr::Table(table_path)) => {
            // Use table's AllColumnsSqlType
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
        Some(QueryableAttr::Select(select_types)) => {
            // Use explicitly specified SQL types
            quote! {
                impl ::diesel_clickhouse::core::deserialize::Queryable<#select_types> for #name {
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
            // No table or select attribute - automatically deduce SQL types from field types
            // using HasSqlType trait: String -> CHString, u64 -> UInt64, Vec<T> -> Array<T::SqlType>, etc.
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

/// Parsed attribute for Queryable derive
enum QueryableAttr {
    Table(syn::Path),
    Select(syn::Type),
}

/// Custom parser for select = (Type1, Type2<Generic>, ...)
struct SelectAttr {
    types: syn::Type,
}

impl Parse for SelectAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        if ident != "select" {
            return Err(syn::Error::new(ident.span(), "expected `select`"));
        }
        input.parse::<Token![=]>()?;
        let types: syn::Type = input.parse()?;
        Ok(SelectAttr { types })
    }
}

/// Parse #[diesel_clickhouse(table = ...)] or #[diesel_clickhouse(select = (...))]
fn get_queryable_attribute(attrs: &[Attribute]) -> Option<QueryableAttr> {
    for attr in attrs {
        if attr.path().is_ident("diesel_clickhouse") {
            // First try to parse as select = (types)
            if let Ok(select_attr) = attr.parse_args::<SelectAttr>() {
                return Some(QueryableAttr::Select(select_attr.types));
            }

            // Then try to parse as table = path
            if let Ok(nested) = attr.parse_args::<syn::Meta>() {
                if let syn::Meta::NameValue(nv) = &nested {
                    if nv.path.is_ident("table") {
                        if let syn::Expr::Path(expr_path) = &nv.value {
                            return Some(QueryableAttr::Table(expr_path.path.clone()));
                        }
                    }
                }
            }
        }
    }
    None
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

#[proc_macro_derive(Insertable, attributes(diesel_clickhouse, column_name))]
pub fn derive_insertable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Get table name from attribute
    let table_path = match require_table_attribute(&input.attrs, "Insertable") {
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
                        diesel_clickhouse::serialize::ToSqlLiteral::to_sql_literal(&self.#field_names),
                    )*
                ]
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

    let table_path = match require_table_attribute(&input.attrs, "Selectable") {
        Ok(path) => path,
        Err(err) => return err,
    };

    let fields = match extract_named_fields(&input, "Selectable") {
        Ok(fields) => fields,
        Err(err) => return err,
    };

    let column_refs: Vec<_> = fields.iter().map(|f| {
        let col_name = get_column_name(f)
            .unwrap_or_else(|| {
                f.ident.as_ref()
                    .expect("Named fields always have identifiers")
                    .to_string()
            });
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

// =============================================================================
// Shared Code Generation Helpers for #[row] and #[typed_row]
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
        let attrs: Vec<Vec<_>> = fields.iter().map(|f| {
            f.attrs.iter().filter(|a| !a.path().is_ident("column_name")).collect()
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

/// Generate typed_row-specific code: column verification and Queryable impl.
fn generate_typed_row_extras(
    name: &Ident,
    table_path: &syn::Path,
    field_info: &RowFieldInfo<'_>,
) -> TokenStream2 {
    let field_names = &field_info.names;
    let field_types = &field_info.types;
    let column_names = &field_info.column_names;

    // Generate field indices for Queryable::build
    let field_indices: Vec<syn::Index> = (0..field_names.len())
        .map(syn::Index::from)
        .collect();

    // Generate column name identifiers for compile-time verification
    let column_idents: Vec<Ident> = column_names.iter()
        .map(|name| format_ident!("{}", name))
        .collect();

    quote! {
        // Compile-time verification that all field/column names exist in the table
        const _: () = {
            #[allow(unused)]
            fn _verify_column_names_for_struct() {
                #(
                    let _ = #table_path::#column_idents;
                )*
            }
        };

        // Generate Queryable implementation for compile-time type checking
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

/// Generate FromNativeBlock, FromAnyBlock, and ToNativeBlock implementations.
fn generate_native_block_impls(name: &Ident, field_info: &RowFieldInfo<'_>) -> TokenStream2 {
    let field_names = &field_info.names;
    let field_types = &field_info.types;
    let column_names = &field_info.column_names;
    let col_var_names = &field_info.col_var_names;

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

        // Generate ToNativeBlock for Native backend (optimized INSERT)
        #[cfg(feature = "native")]
        impl ::diesel_clickhouse::native::ToNativeBlock for #name {
            fn column_names() -> &'static [&'static str] {
                &[#(#column_names),*]
            }

            fn rows_to_block(rows: &[Self]) -> ::diesel_clickhouse::result::QueryResult<::diesel_clickhouse::native::NativeBlock> {
                // Use extend() pattern - allows LLVM to vectorize each column independently
                #(
                    let #col_var_names: <#field_types as ::diesel_clickhouse::native::IntoBlockColumn>::ColumnData =
                        rows.iter()
                            .map(|row| <#field_types as ::diesel_clickhouse::native::IntoBlockColumn>::to_column_value(&row.#field_names))
                            .collect();
                )*

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
