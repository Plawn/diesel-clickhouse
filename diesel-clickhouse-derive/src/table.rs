//! Implementation of the `table!` macro.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, Ident, Token, Type, Result,
    braced, parenthesized,
};

/// Parse a table definition.
#[allow(dead_code)]
struct TableDefinition {
    attrs: Vec<TableAttribute>,
    name: Ident,
    primary_key: Vec<Ident>,
    columns: Vec<ColumnDefinition>,
}

#[allow(dead_code)]
struct TableAttribute {
    name: Ident,
    value: Option<TokenStream2>,
}

struct ColumnDefinition {
    name: Ident,
    sql_type: Type,
}

impl Parse for TableDefinition {
    fn parse(input: ParseStream) -> Result<Self> {
        // Parse optional attributes like #[engine = MergeTree]
        let mut attrs = Vec::with_capacity(4);
        while input.peek(Token![#]) {
            input.parse::<Token![#]>()?;
            let content;
            syn::bracketed!(content in input);
            let name: Ident = content.parse()?;
            let value = if content.peek(Token![=]) {
                content.parse::<Token![=]>()?;
                let tokens: TokenStream2 = content.parse()?;
                Some(tokens)
            } else {
                None
            };
            attrs.push(TableAttribute { name, value });
        }

        // Parse table name
        let name: Ident = input.parse()?;

        // Parse primary key columns (ORDER BY in ClickHouse)
        let pk_content;
        parenthesized!(pk_content in input);
        let primary_key = pk_content
            .parse_terminated(Ident::parse, Token![,])?
            .into_iter()
            .collect();

        // Parse column definitions
        let columns_content;
        braced!(columns_content in input);
        let mut columns = Vec::with_capacity(16);

        while !columns_content.is_empty() {
            let col_name: Ident = columns_content.parse()?;
            columns_content.parse::<Token![->]>()?;
            let sql_type: Type = columns_content.parse()?;

            // Optional trailing comma
            if columns_content.peek(Token![,]) {
                columns_content.parse::<Token![,]>()?;
            }

            columns.push(ColumnDefinition {
                name: col_name,
                sql_type,
            });
        }

        Ok(TableDefinition {
            attrs,
            name,
            primary_key,
            columns,
        })
    }
}

pub fn table_impl(input: TokenStream) -> TokenStream {
    let definition = parse_macro_input!(input as TableDefinition);

    let table_name = &definition.name;
    let table_name_str = table_name.to_string();
    let primary_key_columns = &definition.primary_key;
    let columns = &definition.columns;

    // Generate column struct and impl for each column
    let column_defs: Vec<TokenStream2> = columns.iter().map(|col| {
        let col_name = &col.name;
        let col_name_str = col_name.to_string();
        let sql_type = &col.sql_type;

        quote! {
            /// Column `#col_name_str`.
            #[allow(non_camel_case_types)]
            #[derive(Debug, Clone, Copy, Default)]
            pub struct #col_name;

            impl diesel_clickhouse_core::expression::Expression for #col_name {
                type SqlType = #sql_type;
            }

            impl diesel_clickhouse_core::query_source::Column for #col_name {
                type Table = table;

                fn column_name() -> &'static str {
                    #col_name_str
                }
            }

            impl diesel_clickhouse_core::expression::SelectableExpression<table> for #col_name {}
            impl diesel_clickhouse_core::expression::AppearsOnTable<table> for #col_name {}

            impl<DB: diesel_clickhouse_core::backend::Backend> diesel_clickhouse_core::query_builder::QueryFragment<DB> for #col_name {
                fn walk_ast<'b>(&'b self, mut pass: diesel_clickhouse_core::query_builder::AstPass<'_, 'b, DB>) -> diesel_clickhouse_core::result::QueryResult<()> {
                    pass.push_identifier(#col_name_str);
                    Ok(())
                }
            }

            // Note: ExpressionMethods is implemented via blanket impl in diesel_clickhouse_core
        }
    }).collect();

    // Column names for all_columns tuple
    let column_names: Vec<&Ident> = columns.iter().map(|c| &c.name).collect();
    let column_types: Vec<&Type> = columns.iter().map(|c| &c.sql_type).collect();
    let column_count = columns.len();

    // Primary key tuple
    let pk_tuple = if primary_key_columns.len() == 1 {
        let pk = &primary_key_columns[0];
        quote! { #pk }
    } else {
        quote! { (#(#primary_key_columns),*) }
    };

    // Generate the module
    let expanded = quote! {
        #[allow(non_snake_case, dead_code, unused_imports)]
        pub mod #table_name {
            use super::*;
            use diesel_clickhouse_core::prelude::*;

            /// The table struct.
            #[derive(Debug, Clone, Copy, Default)]
            pub struct table;

            impl diesel_clickhouse_core::query_source::Table for table {
                type PrimaryKey = #pk_tuple;
                type AllColumns = (#(#column_names,)*);
                type AllColumnsSqlType = (#(#column_types,)*);

                fn table_name() -> &'static str {
                    #table_name_str
                }

                fn all_columns() -> Self::AllColumns {
                    (#(#column_names,)*)
                }

                fn primary_key() -> Self::PrimaryKey {
                    #pk_tuple
                }
            }

            impl diesel_clickhouse_core::query_source::QuerySource for table {
                type FromClause = Self;
                type DefaultSelection = <Self as diesel_clickhouse_core::query_source::Table>::AllColumns;

                fn from_clause(&self) -> Self::FromClause {
                    *self
                }

                fn default_selection(&self) -> Self::DefaultSelection {
                    Self::all_columns()
                }
            }

            impl<DB: diesel_clickhouse_core::backend::Backend> diesel_clickhouse_core::query_builder::QueryFragment<DB> for table {
                fn walk_ast<'b>(&'b self, mut pass: diesel_clickhouse_core::query_builder::AstPass<'_, 'b, DB>) -> diesel_clickhouse_core::result::QueryResult<()> {
                    pass.push_identifier(#table_name_str);
                    Ok(())
                }
            }

            // Note: QueryDsl and ClickHouseQueryDsl are implemented via blanket impls in diesel_clickhouse_core

            // Column definitions
            #(#column_defs)*

            /// All columns in this table.
            pub const all_columns: (#(#column_names,)*) = (#(#column_names,)*);

            /// The star (*) expression.
            pub const star: diesel_clickhouse_core::expression::Star<table> = diesel_clickhouse_core::expression::Star::new();

            /// DSL re-exports for convenient imports.
            pub mod dsl {
                pub use super::table;
                #(pub use super::#column_names;)*
                pub use super::star;
                pub use super::all_columns;
            }

            /// Column helper functions.
            pub mod columns {
                use super::*;

                /// Get all column names.
                pub fn names() -> &'static [&'static str] {
                    static NAMES: [&str; #column_count] = [
                        #(stringify!(#column_names)),*
                    ];
                    &NAMES
                }
            }
        }
    };

    TokenStream::from(expanded)
}
