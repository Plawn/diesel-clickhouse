//! Unified row trait for diesel-clickhouse.
//!
//! This module provides the [`SerdeRow`] trait which abstracts over
//! the different row representations used by the HTTP and Native backends.
//!
//! # Usage
//!
//! Use `#[derive(ClickHouseRow)]` to automatically implement all necessary
//! traits for both backends:
//!
//! ```rust,ignore
//! use diesel_clickhouse::prelude::*;
//!
//! #[derive(Debug, ClickHouseRow)]
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
//! `#[derive(ClickHouseRow)]` generates:
//!
//! 1. `serde::Serialize` and `serde::Deserialize` - for HTTP backend (RowBinary format)
//! 2. `clickhouse::Row` - for HTTP backend (clickhouse crate) when `http` feature is enabled
//! 3. `FromNativeBlock` / `ToNativeBlock` - for Native backend when `native` feature is enabled
//!
//! This allows a single struct definition to work seamlessly with both connection types.

use serde::{Serialize, de::DeserializeOwned};

/// Marker trait for types that support serde-based serialization.
///
/// This trait is automatically satisfied by any type implementing
/// `Serialize + DeserializeOwned + Send + 'static`. It indicates that
/// the type can be used with serde-based backends.
///
/// For full backend support (HTTP + Native), use `#[derive(ClickHouseRow)]`
/// which generates all necessary impls including `clickhouse::Row`,
/// `Serialize`, `Deserialize`, and native block traits.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::prelude::*;
///
/// #[derive(Debug, ClickHouseRow)]
/// struct Event {
///     id: u64,
///     name: String,
///     timestamp: u32,
/// }
///
/// // Event now implements SerdeRow and works with all backends
/// ```
pub trait SerdeRow: Serialize + DeserializeOwned + Send + Sized + 'static {}

/// Blanket implementation for any type that satisfies the requirements.
impl<T> SerdeRow for T where T: Serialize + DeserializeOwned + Send + Sized + 'static {}

/// Trait for types that can be inserted into ClickHouse.
///
/// This is a subset of `SerdeRow` for insert-only types.
pub trait InsertableRow: Serialize + Send + Sized + 'static {}

/// Blanket implementation for insertable rows.
impl<T> InsertableRow for T where T: Serialize + Send + Sized + 'static {}

/// Trait for types that can be queried from ClickHouse.
///
/// This is a subset of `SerdeRow` for query-only types.
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
    fn test_serde_row_impl() {
        fn assert_serde_row<T: SerdeRow>() {}
        assert_serde_row::<TestRow>();
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
