// Proc-macro crates: panics occur at compile-time, not runtime, so they're acceptable.
// We still deny unwrap/expect but allow specific cases with documented invariants.
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Allow in proc-macro code where panics produce compile errors (acceptable)
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
use syn::{parse_macro_input, DeriveInput, Data, Fields, Ident, LitStr, Attribute, ItemStruct};
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
    let where_clause = &generics.where_clause;

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

    // Collect field info
    let field_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();
    let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();
    let field_attrs: Vec<Vec<_>> = fields.iter().map(|f| {
        // Keep non-column_name attributes
        f.attrs.iter().filter(|a| !a.path().is_ident("column_name")).collect()
    }).collect();
    let field_vis: Vec<_> = fields.iter().map(|f| &f.vis).collect();

    let column_names: Vec<String> = fields.iter()
        .map(|f| get_column_name(f).unwrap_or_else(|| {
            f.ident.as_ref()
                .expect("Named fields always have identifiers")
                .to_string()
        }))
        .collect();

    // Generate serde rename attributes for fields that have different column names
    let serde_renames: Vec<_> = column_names.iter().zip(field_names.iter()).map(|(col, field)| {
        let field_str = field.as_ref()
            .expect("Named fields always have identifiers")
            .to_string();
        if col != &field_str {
            quote! { #[serde(rename = #col)] }
        } else {
            quote! {}
        }
    }).collect();

    // Generate column vector variable names for ToNativeBlock
    let col_var_names: Vec<_> = field_names.iter()
        .map(|f| format_ident!("col_{}", f.as_ref().expect("Named fields always have identifiers")))
        .collect();

    let expanded = quote! {
        // Re-emit the struct with clickhouse::Row derive for HTTP backend
        #(#attrs)*
        #[cfg_attr(feature = "http", derive(::diesel_clickhouse::clickhouse::Row))]
        #[cfg_attr(feature = "http", derive(::serde::Deserialize))]
        #vis struct #name #generics #where_clause {
            #(
                #(#field_attrs)*
                #serde_renames
                #field_vis #field_names: #field_types,
            )*
        }

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
                // Create column vectors
                #(
                    let mut #col_var_names: <#field_types as ::diesel_clickhouse::native::IntoBlockColumn>::ColumnData =
                        <#field_types as ::diesel_clickhouse::native::IntoBlockColumn>::new_column();
                )*

                // Populate column vectors
                for row in rows {
                    #(
                        <#field_types as ::diesel_clickhouse::native::IntoBlockColumn>::push_to_column(&row.#field_names, &mut #col_var_names);
                    )*
                }

                // Build block
                let block = ::diesel_clickhouse::native::NativeBlock::new();
                #(
                    let block = <#field_types as ::diesel_clickhouse::native::IntoBlockColumn>::add_column_to_block(block, #column_names, #col_var_names);
                )*

                Ok(block)
            }
        }
    };

    TokenStream::from(expanded)
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

    let fields = match extract_named_fields(&input, "Queryable") {
        Ok(fields) => fields,
        Err(err) => return err,
    };

    let field_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();
    let field_names_str: Vec<_> = fields.iter()
        .map(|f| get_column_name(f).unwrap_or_else(|| {
            f.ident.as_ref()
                .expect("Named fields always have identifiers")
                .to_string()
        }))
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
    let table_path = match require_table_attribute(&input.attrs, "Insertable") {
        Ok(path) => path,
        Err(err) => return err,
    };

    let fields = match extract_named_fields(&input, "Insertable") {
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
