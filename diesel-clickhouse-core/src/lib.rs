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

pub mod backend;
pub mod result;
pub mod expression;
pub mod query_source;
pub mod query_builder;
pub mod query_dsl;
pub mod connection;
pub mod deserialize;
pub mod serialize;

// Re-export types crate
pub use diesel_clickhouse_types as types;

// Re-export core items
pub use backend::{Backend, HttpBackend, NativeBackend, BindValue};
pub use result::{Error, QueryResult};
pub use expression::{Expression, SelectableExpression, AppearsOnTable, BoxableExpression};
pub use query_source::{Table, Column, QuerySource};
pub use query_builder::{QueryFragment, AstPass, update, delete, UpdateStatement, DeleteStatement, AsChangeset, Assign, Assignments};
pub use backend::QueryBuilder;
pub use query_dsl::{QueryDsl, ClickHouseQueryDsl, RunQueryDsl, FindStatement};
pub use connection::AsyncConnection;
pub use deserialize::FromRow;
pub use serialize::ToRow;

/// Prelude for common imports.
pub mod prelude {
    pub use super::backend::{Backend, HttpBackend, NativeBackend};
    pub use super::result::{Error, QueryResult};
    pub use super::expression::{Expression, SelectableExpression, ExpressionMethods};
    pub use super::query_source::{Table, Column, QuerySource};
    pub use super::query_dsl::{QueryDsl, ClickHouseQueryDsl, RunQueryDsl};
    pub use super::connection::AsyncConnection;
    pub use super::deserialize::FromRow;
    pub use super::serialize::ToRow;

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
