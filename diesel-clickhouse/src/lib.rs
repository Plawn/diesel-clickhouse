// Deny unwrap/expect in library code to prevent panics
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Allow in tests
#![cfg_attr(test, allow(clippy::unwrap_used))]
#![cfg_attr(test, allow(clippy::expect_used))]

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
//! async fn query_events(conn: &Connection) -> Result<Vec<Event>, Error> {
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
pub use diesel_clickhouse_derive::{table, Queryable, Insertable, Selectable, Row, row};

// Re-export clickhouse crate for Row derive to use
#[cfg(feature = "http")]
pub use clickhouse;

// Re-export key items at the top level
pub use core::backend::{Backend, HttpBackend, NativeBackend, ClickHouse};
pub use core::result::{Error, QueryResult};
pub use core::expression::{Expression, SelectableExpression, ExpressionMethods};
pub use core::query_source::{Table, Column, QuerySource};
pub use core::query_builder::{QueryFragment, insert_into, update, delete, UpdateStatement, DeleteStatement, AsChangeset, Assign, Assignments, Insertable as InsertableTrait};
pub use core::query_dsl::{QueryDsl, ClickHouseQueryDsl};
// Re-export the unified connection trait with a cleaner name
pub use core::connection::ClickHouseConnection as ClickHouseConnectionTrait;
pub use core::deserialize::FromRow;
pub use core::serialize::ToRow;
pub use core::row::{ClickHouseRow, InsertableRow, QueryableRow};

/// Prelude module for common imports.
///
/// Use `use diesel_clickhouse::prelude::*;` to import commonly used items.
pub mod prelude {
    // Core traits
    pub use crate::core::prelude::*;

    // Unified connection trait (renamed to avoid conflict with http::ClickHouseConnection)
    pub use crate::core::connection::ClickHouseConnection as ClickHouseConnectionTrait;
    pub use crate::core::row::{ClickHouseRow, InsertableRow, QueryableRow};

    // Derive macros
    pub use crate::{table, Queryable, Insertable, Selectable, Row, row};

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
    pub use crate::http::{InsertDsl, ToSql};

    // Native execution traits
    #[cfg(all(feature = "native", not(feature = "http")))]
    pub use crate::native::ToSql;

    // Unified connection
    #[cfg(any(feature = "http", feature = "native"))]
    pub use crate::Connection;

    // RunQueryDsl for idiomatic query execution
    #[cfg(any(feature = "http", feature = "native"))]
    pub use crate::RunQueryDsl;
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
///
/// Uses ClickHouse's HTTP interface (port 8123).
/// This is the default backend and is easier to use in most environments.
#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "http")]
pub use http::ClickHouseConnection;

#[cfg(feature = "http")]
pub use http::{HttpClientBuilder, Compression};

/// Native protocol connection module.
///
/// Uses ClickHouse's native binary protocol (port 9000/9440 for TLS).
/// This is faster than HTTP but requires direct TCP connectivity.
///
/// # Features
///
/// - `native` - Enable the native backend
/// - `native-tls-native` - Enable TLS support (uses rustls)
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::native::{NativeConnection, NativeConnectionOptions};
///
/// // Plain TCP connection (port 9000)
/// let conn = NativeConnection::establish("localhost:9000", Default::default()).await?;
///
/// // With TLS (port 9440)
/// #[cfg(feature = "native-tls-native")]
/// {
///     use diesel_clickhouse::native::TlsConfig;
///     let tls = TlsConfig::new()?;
///     let conn = NativeConnection::establish_tls(
///         "localhost:9440",
///         "localhost",
///         tls,
///         Default::default(),
///     ).await?;
/// }
/// ```
#[cfg(feature = "native")]
pub mod native;

#[cfg(feature = "native")]
pub use native::NativeConnection;

#[cfg(feature = "native")]
pub use native::{NativeClientBuilder, NativeCompression};

/// Unified connection interface.
///
/// Provides a single API that works with both HTTP and Native backends.
/// The backend is selected automatically based on the URL scheme.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::Connection;
///
/// // HTTP backend
/// let conn = Connection::establish("http://localhost:8123/default").await?;
///
/// // Native backend
/// let conn = Connection::establish("tcp://localhost:9000/default").await?;
///
/// // Same API for both
/// conn.execute("CREATE TABLE test (id UInt64) ENGINE = Memory").await?;
/// conn.insert_values("test", "(1), (2), (3)").await?;
/// ```
#[cfg(any(feature = "http", feature = "native"))]
mod unified;

#[cfg(any(feature = "http", feature = "native"))]
pub use unified::Connection;

/// RunQueryDsl for idiomatic Diesel-style query execution.
///
/// Allows calling `.load()`, `.first()`, `.execute()` directly on queries.
#[cfg(any(feature = "http", feature = "native"))]
mod run_query_dsl;

#[cfg(any(feature = "http", feature = "native"))]
pub use run_query_dsl::RunQueryDsl;

/// Unified streaming interface for query results.
///
/// Provides `RowStream` for processing large result sets row by row.
#[cfg(any(feature = "http", feature = "native"))]
pub mod stream;

#[cfg(any(feature = "http", feature = "native"))]
pub use stream::RowStream;

/// Zero-copy columnar data processing with Apache Arrow.
///
/// This module provides high-performance data loading using Apache Arrow's
/// columnar format. Arrow enables true zero-copy data access and is optimized
/// for analytical workloads.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::prelude::*;
/// use diesel_clickhouse::arrow::array::{Int64Array, StringArray};
///
/// // Load data as Arrow RecordBatches
/// let result = conn.load_arrow("SELECT id, name FROM users").await?;
///
/// for batch in result {
///     let ids = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
///     let names = batch.column(1).as_any().downcast_ref::<StringArray>().unwrap();
///
///     for i in 0..batch.num_rows() {
///         println!("User {}: {}", ids.value(i), names.value(i));
///     }
/// }
/// ```
#[cfg(feature = "arrow")]
pub mod arrow;

#[cfg(feature = "arrow")]
pub use arrow::{ArrowResult, ArrowRow, parse_arrow_stream, build_column_index, for_each_row};

/// Batch insertion utilities.
///
/// Provides `BatchInserter` for efficient bulk inserts.
#[cfg(any(feature = "http", feature = "native"))]
pub mod batch;

#[cfg(any(feature = "http", feature = "native"))]
pub use batch::{BatchInserter, RawBatchInserter, BatchInsertExt};

/// Connection pooling.
///
/// Provides `Pool` for efficient connection reuse.
#[cfg(any(feature = "http", feature = "native"))]
pub mod pool;

#[cfg(any(feature = "http", feature = "native"))]
pub use pool::{Pool, PoolConfig, PooledConnection};

/// Async insert mode support.
///
/// Provides `AsyncInserter` for high-throughput inserts using ClickHouse's
/// async_insert mode.
#[cfg(any(feature = "http", feature = "native"))]
pub mod async_insert;

#[cfg(any(feature = "http", feature = "native"))]
pub use async_insert::{AsyncInsertConfig, AsyncInserter, BufferedAsyncInserter, AsyncInsertExt};

/// Migrations module.
///
/// Re-exports from diesel-clickhouse-migrations when the `migrations` feature is enabled.
#[cfg(feature = "migrations")]
pub mod migrations {
    pub use diesel_clickhouse_migrations::*;
}
