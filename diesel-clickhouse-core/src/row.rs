//! Unified row trait for diesel-clickhouse.
//!
//! This module provides the [`ClickHouseRow`] trait which abstracts over
//! the different row representations used by the HTTP and Native backends.
//!
//! # Usage
//!
//! Use the `#[derive(Row)]` macro to automatically implement all necessary
//! traits for both backends:
//!
//! ```rust,ignore
//! use diesel_clickhouse::Row;
//!
//! #[derive(Debug, Row)]
//! struct User {
//!     id: u64,
//!     name: String,
//!     email: Option<String>,
//! }
//!
//! // Works with both HTTP and Native connections
//! let users: Vec<User> = conn.load(users::table.filter(users::active.eq(true))).await?;
//! ```
//!
//! # How It Works
//!
//! The `#[derive(Row)]` macro generates:
//!
//! 1. `serde::Serialize` and `serde::Deserialize` - for Native backend (clickhouse-rs)
//! 2. `clickhouse::Row` - for HTTP backend (clickhouse crate) when `http` feature is enabled
//! 3. `ClickHouseRow` marker trait - to indicate the type supports both backends
//!
//! This allows a single struct definition to work seamlessly with both connection types.

use serde::{Serialize, de::DeserializeOwned};

/// Marker trait for types that can be fetched from ClickHouse.
///
/// This trait is automatically implemented by the `#[derive(Row)]` macro.
/// It ensures that a type has all the necessary implementations to work
/// with both HTTP and Native backends.
///
/// # Requirements
///
/// A type implementing `ClickHouseRow` must also implement:
/// - `serde::Serialize` - for serialization (inserts)
/// - `serde::Deserialize` - for deserialization (queries)
/// - `clickhouse::Row` (when `http` feature is enabled) - for HTTP backend
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::Row;
///
/// #[derive(Debug, Row)]
/// struct Event {
///     id: u64,
///     name: String,
///     timestamp: u32,
/// }
///
/// // Now Event implements ClickHouseRow and works with all backends
/// ```
pub trait ClickHouseRow: Serialize + DeserializeOwned + Send + Sized + 'static {}

/// Blanket implementation for any type that satisfies the requirements.
impl<T> ClickHouseRow for T where T: Serialize + DeserializeOwned + Send + Sized + 'static {}

/// Trait for types that can be inserted into ClickHouse.
///
/// This is a subset of `ClickHouseRow` for insert-only types.
pub trait InsertableRow: Serialize + Send + Sized + 'static {}

/// Blanket implementation for insertable rows.
impl<T> InsertableRow for T where T: Serialize + Send + Sized + 'static {}

/// Trait for types that can be queried from ClickHouse.
///
/// This is a subset of `ClickHouseRow` for query-only types.
pub trait QueryableRow: DeserializeOwned + Send + Sized + 'static {}

/// Blanket implementation for queryable rows.
impl<T> QueryableRow for T where T: DeserializeOwned + Send + Sized + 'static {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct TestRow {
        id: u64,
        name: String,
    }

    #[test]
    fn test_clickhouse_row_impl() {
        fn assert_clickhouse_row<T: ClickHouseRow>() {}
        assert_clickhouse_row::<TestRow>();
    }

    #[test]
    fn test_insertable_row_impl() {
        fn assert_insertable<T: InsertableRow>() {}
        assert_insertable::<TestRow>();
    }

    #[test]
    fn test_queryable_row_impl() {
        fn assert_queryable<T: QueryableRow>() {}
        assert_queryable::<TestRow>();
    }
}
