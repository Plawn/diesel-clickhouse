//! # diesel-clickhouse
//!
//! A type-safe, Diesel-inspired ORM for ClickHouse.
//!
//! This library provides compile-time query validation and a familiar
//! query builder API for ClickHouse, inspired by the Diesel ORM for
//! PostgreSQL/MySQL/SQLite.
//!
//! ## Features
//!
//! - **Type-safe query building**: Queries are validated at compile time
//! - **Async-first**: Built on tokio for async operations
//! - **Dual-protocol support**: HTTP and Native protocol via traits
//! - **ClickHouse-specific features**: FINAL, PREWHERE, SAMPLE, ARRAY JOIN
//! - **Batch inserts**: Optimized for ClickHouse's batch-oriented design
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use diesel_clickhouse::prelude::*;
//!
//! // Define your table schema
//! diesel_clickhouse::table! {
//!     events (id, timestamp) {
//!         id -> UInt64,
//!         user_id -> UInt32,
//!         event_type -> LowCardinality<CHString>,
//!         timestamp -> DateTime64<3>,
//!     }
//! }
//!
//! // Define your row types
//! #[derive(Queryable)]
//! struct Event {
//!     id: u64,
//!     user_id: u32,
//!     event_type: String,
//!     timestamp: chrono::NaiveDateTime,
//! }
//!
//! #[derive(Insertable)]
//! #[diesel_clickhouse(table = events)]
//! struct NewEvent {
//!     id: u64,
//!     user_id: u32,
//!     event_type: String,
//!     timestamp: chrono::NaiveDateTime,
//! }
//!
//! // Query with ClickHouse-specific features
//! async fn query_events(conn: &mut impl AsyncConnection) -> Result<Vec<Event>, Error> {
//!     events::table
//!         .filter(events::user_id.eq(42))
//!         .prewhere(events::timestamp.gt(now() - days(7)))
//!         .final_()
//!         .order_by(events::timestamp.desc())
//!         .limit(100)
//!         .load(conn)
//!         .await
//! }
//! ```
//!
//! ## ClickHouse-Specific Features
//!
//! ### FINAL Modifier
//!
//! Forces deduplication for ReplacingMergeTree tables:
//!
//! ```rust,ignore
//! users::table.final_().load(&mut conn).await?;
//! ```
//!
//! ### PREWHERE Clause
//!
//! Optimized pre-filtering (reads fewer columns):
//!
//! ```rust,ignore
//! events::table
//!     .prewhere(events::user_id.eq(42))
//!     .filter(events::status.eq("active"))
//!     .load(&mut conn).await?;
//! ```
//!
//! ### SAMPLE Clause
//!
//! Approximate queries on a subset of data:
//!
//! ```rust,ignore
//! // Query 10% of the data
//! events::table.sample(0.1).load(&mut conn).await?;
//! ```
//!
//! ### ARRAY JOIN
//!
//! Flatten arrays into rows:
//!
//! ```rust,ignore
//! articles::table
//!     .array_join(articles::tags)
//!     .select((articles::id, articles::tags))
//!     .load(&mut conn).await?;
//! ```

// Re-export core crate
pub use diesel_clickhouse_core as core;

// Re-export types crate
pub use diesel_clickhouse_types as types;

// Re-export derive macros
pub use diesel_clickhouse_derive::{table, Queryable, Insertable, Selectable};

// Re-export key items at the top level
pub use core::backend::{Backend, HttpBackend, NativeBackend, ClickHouse};
pub use core::result::{Error, QueryResult};
pub use core::expression::{Expression, SelectableExpression, ExpressionMethods};
pub use core::query_source::{Table, Column, QuerySource};
pub use core::query_builder::{QueryFragment, insert_into, update, delete, UpdateStatement, DeleteStatement, AsChangeset, Assign, Assignments, Insertable as InsertableTrait};
pub use core::query_dsl::{QueryDsl, ClickHouseQueryDsl, RunQueryDsl};
pub use core::connection::AsyncConnection;
pub use core::deserialize::FromRow;
pub use core::serialize::ToRow;

/// Prelude module for common imports.
///
/// Use `use diesel_clickhouse::prelude::*;` to import commonly used items.
pub mod prelude {
    // Core traits
    pub use crate::core::prelude::*;

    // Derive macros
    pub use crate::{table, Queryable, Insertable, Selectable};

    // Common functions
    pub use crate::core::expression::functions::{
        count, count_star, sum, avg, min, max,
        group_array, uniq, array_length, has,
        now, today, to_string, coalesce,
    };

    // Query helpers
    pub use crate::core::query_builder::{insert_into, update, delete};

    // Expression methods
    pub use crate::core::expression::methods::{
        ExpressionMethods,
        NullableExpressionMethods,
        BoolExpressionMethods,
        TextExpressionMethods,
        OrderExpressionMethods,
    };

    // HTTP execution traits
    #[cfg(feature = "http")]
    pub use crate::http::{ExecuteMut, InsertDsl, ToSql};
}

/// DSL helpers and functions.
pub mod dsl {
    pub use crate::core::expression::functions::*;
    pub use crate::core::expression::sql;
}

/// Query building types.
pub mod query_builder {
    pub use crate::core::query_builder::*;
}

/// Expression types and traits.
pub mod expression {
    pub use crate::core::expression::*;
}

/// Result and error types.
pub mod result {
    pub use crate::core::result::*;
}

/// Connection types.
pub mod connection {
    pub use crate::core::connection::*;
}

/// Serialization and deserialization.
pub mod serialize {
    pub use crate::core::serialize::*;
}

pub mod deserialize {
    pub use crate::core::deserialize::*;
}

/// Backend types.
pub mod backend {
    pub use crate::core::backend::*;
}

/// HTTP connection module.
#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "http")]
pub use http::ClickHouseConnection;
