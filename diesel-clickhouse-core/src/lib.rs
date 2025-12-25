// Deny unwrap/expect in library code to prevent panics
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Allow in tests
#![cfg_attr(test, allow(clippy::unwrap_used))]
#![cfg_attr(test, allow(clippy::expect_used))]

//! Core traits and types for diesel-clickhouse.
//!
//! This crate provides the fundamental abstractions for building type-safe
//! queries against ClickHouse:
//!
//! - [`Backend`] - Abstraction over HTTP and Native protocols
//! - [`Expression`] - Base trait for all SQL expressions
//! - [`Table`] and [`Column`] - Schema representation
//! - [`QueryDsl`] - Query building methods
//! - [`AsyncConnection`] - Async database connection
//! - [`ClickHouseRow`] - Unified row trait for both HTTP and Native backends

pub mod backend;
pub mod result;
pub mod expression;
pub mod query_source;
pub mod query_builder;
pub mod query_dsl;
pub mod connection;
pub mod deserialize;
pub mod serialize;
pub mod row;

/// SQL escaping utilities for preventing SQL injection.
pub mod escape;

/// Arena allocator for efficient query building.
pub mod arena;

/// String interning for column names.
pub mod interner;

/// ClickHouse type string parser (for schema introspection).
pub mod type_parser;

/// SQL building utilities (shared across backends).
pub mod sql_builder;

// Re-export types crate
pub use diesel_clickhouse_types as types;

// Re-export core items
pub use backend::{Backend, HttpBackend, NativeBackend};
pub use result::{Error, QueryResult};
pub use expression::{Expression, SelectableExpression, AppearsOnTable, BoxableExpression};
pub use query_source::{Table, Column, QuerySource};
pub use query_builder::{QueryFragment, AstPass, QueryOutputType, HasAllColumnsSqlType, update, delete, UpdateStatement, DeleteStatement, AsChangeset, Assign, Assignments};
pub use backend::QueryBuilder;
pub use query_dsl::{QueryDsl, ClickHouseQueryDsl, RunQueryDsl, FindStatement};
pub use connection::ClickHouseConnection;
pub use deserialize::{FromRow, Queryable};
pub use serialize::ToRow;
pub use row::{ClickHouseRow, InsertableRow, QueryableRow};
pub use type_parser::{parse_type, ClickHouseSqlType, ColumnInfo, TableInfo};
pub use sql_builder::{build_sql, build_sql_with_bindings, ToSqlString, BindableValue};

/// Prelude for common imports.
pub mod prelude {
    pub use super::backend::{Backend, HttpBackend, NativeBackend};
    pub use super::result::{Error, QueryResult};
    pub use super::expression::{Expression, SelectableExpression, ExpressionMethods};
    pub use super::query_source::{Table, Column, QuerySource, JoinDsl, ArrayJoinDsl};
    pub use super::query_dsl::{QueryDsl, ClickHouseQueryDsl, RunQueryDsl};
    pub use super::query_builder::AliasDsl;
    pub use super::connection::ClickHouseConnection;
    pub use super::deserialize::FromRow;
    pub use super::serialize::ToRow;
    pub use super::row::{ClickHouseRow, InsertableRow, QueryableRow};

    // Re-export common types
    pub use diesel_clickhouse_types::{
        SqlType, HasSqlType,
        // Integers
        UInt8, UInt16, UInt32, UInt64, UInt128, UInt256,
        Int8, Int16, Int32, Int64, Int128, Int256,
        Bool, U256, I256,
        // Floats
        Float32, Float64, Decimal, Decimal32, Decimal64, Decimal128,
        // Strings
        CHString, FixedString, UUID, IPv4, IPv6,
        // Temporal
        Date, Date32, DateTime, DateTime64,
        // Complex
        Array, Map, Tuple, Nested, LowCardinality, Enum8, Enum16,
        // Nullable
        Nullable,
    };
}
