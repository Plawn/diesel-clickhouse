//! RunQueryDsl implementation for idiomatic Diesel-style query execution.
//!
//! This module provides the `RunQueryDsl` trait implementation that allows
//! calling `.load()`, `.first()`, `.execute()` etc. directly on queries.
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse::prelude::*;
//!
//! #[row]
//! #[derive(Debug)]
//! struct User {
//!     id: u64,
//!     name: String,
//! }
//!
//! // Idiomatic Diesel style:
//! let users: Vec<User> = users::table
//!     .filter(users::active.eq(true))
//!     .order_by(users::name.asc())
//!     .load(&conn)
//!     .await?;
//!
//! // Get first result:
//! let user: User = users::table
//!     .filter(users::id.eq(42))
//!     .first(&conn)
//!     .await?;
//!
//! // Execute mutation:
//! insert_into(users::table)
//!     .values(&new_user)
//!     .execute(&conn)
//!     .await?;
//! ```

use crate::core::backend::ClickHouse;
use crate::core::query_builder::QueryFragment;
use crate::core::result::{Error, QueryResult};
use crate::Connection;

/// Extension trait for executing queries in idiomatic Diesel style.
///
/// This trait is automatically implemented for all types that implement
/// `QueryFragment<ClickHouse>`, allowing you to call `.load()`, `.first()`,
/// `.get_result()`, and `.execute()` directly on queries.
///
/// Row types must be marked with `#[row]` for optimized binary deserialization.
#[allow(async_fn_in_trait)]
pub trait RunQueryDsl: Sized {
    /// Execute the query and load all results.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let users: Vec<User> = users::table
    ///     .filter(users::active.eq(true))
    ///     .load(&conn)
    ///     .await?;
    /// ```
    #[cfg(all(feature = "http", not(feature = "native")))]
    async fn load<U>(self, conn: &Connection) -> QueryResult<Vec<U>>
    where
        U: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send;

    #[cfg(all(feature = "native", not(feature = "http")))]
    async fn load<U>(self, conn: &Connection) -> QueryResult<Vec<U>>
    where
        U: crate::native::FromNativeBlock + Send;

    #[cfg(all(feature = "http", feature = "native"))]
    async fn load<U>(self, conn: &Connection) -> QueryResult<Vec<U>>
    where
        U: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromNativeBlock + Send;

    /// Execute the query and return the first result.
    ///
    /// Returns an error if no rows are found.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: User = users::table
    ///     .filter(users::id.eq(42))
    ///     .first(&conn)
    ///     .await?;
    /// ```
    #[cfg(all(feature = "http", not(feature = "native")))]
    async fn first<U>(self, conn: &Connection) -> QueryResult<U>
    where
        U: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send;

    #[cfg(all(feature = "native", not(feature = "http")))]
    async fn first<U>(self, conn: &Connection) -> QueryResult<U>
    where
        U: crate::native::FromNativeBlock + Send;

    #[cfg(all(feature = "http", feature = "native"))]
    async fn first<U>(self, conn: &Connection) -> QueryResult<U>
    where
        U: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromNativeBlock + Send;

    /// Execute the query and return an optional result.
    ///
    /// Returns `Ok(None)` if no rows are found.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let user: Option<User> = users::table
    ///     .filter(users::id.eq(42))
    ///     .get_result(&conn)
    ///     .await?;
    /// ```
    #[cfg(all(feature = "http", not(feature = "native")))]
    async fn get_result<U>(self, conn: &Connection) -> QueryResult<Option<U>>
    where
        U: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send;

    #[cfg(all(feature = "native", not(feature = "http")))]
    async fn get_result<U>(self, conn: &Connection) -> QueryResult<Option<U>>
    where
        U: crate::native::FromNativeBlock + Send;

    #[cfg(all(feature = "http", feature = "native"))]
    async fn get_result<U>(self, conn: &Connection) -> QueryResult<Option<U>>
    where
        U: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromNativeBlock + Send;

    /// Execute the query (for INSERT, UPDATE, DELETE).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// insert_into(users::table)
    ///     .values(&new_user)
    ///     .execute(&conn)
    ///     .await?;
    /// ```
    async fn execute(self, conn: &Connection) -> QueryResult<()>;
}

// =============================================================================
// HTTP-only implementation
// =============================================================================

#[cfg(all(feature = "http", not(feature = "native")))]
impl<T> RunQueryDsl for T
where
    T: QueryFragment<ClickHouse> + Send,
{
    async fn load<U>(self, conn: &Connection) -> QueryResult<Vec<U>>
    where
        U: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
    {
        conn.load(self).await
    }

    async fn first<U>(self, conn: &Connection) -> QueryResult<U>
    where
        U: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
    {
        conn.load(self).await?.into_iter().next().ok_or(Error::NotFound)
    }

    async fn get_result<U>(self, conn: &Connection) -> QueryResult<Option<U>>
    where
        U: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send,
    {
        Ok(conn.load(self).await?.into_iter().next())
    }

    async fn execute(self, conn: &Connection) -> QueryResult<()> {
        conn.execute_query(self).await
    }
}

// =============================================================================
// Native-only implementation
// =============================================================================

#[cfg(all(feature = "native", not(feature = "http")))]
impl<T> RunQueryDsl for T
where
    T: QueryFragment<ClickHouse> + Send,
{
    async fn load<U>(self, conn: &Connection) -> QueryResult<Vec<U>>
    where
        U: crate::native::FromNativeBlock + Send,
    {
        conn.load(self).await
    }

    async fn first<U>(self, conn: &Connection) -> QueryResult<U>
    where
        U: crate::native::FromNativeBlock + Send,
    {
        conn.load(self).await?.into_iter().next().ok_or(Error::NotFound)
    }

    async fn get_result<U>(self, conn: &Connection) -> QueryResult<Option<U>>
    where
        U: crate::native::FromNativeBlock + Send,
    {
        Ok(conn.load(self).await?.into_iter().next())
    }

    async fn execute(self, conn: &Connection) -> QueryResult<()> {
        conn.execute_query(self).await
    }
}

// =============================================================================
// Both HTTP and Native implementation
// =============================================================================

#[cfg(all(feature = "http", feature = "native"))]
impl<T> RunQueryDsl for T
where
    T: QueryFragment<ClickHouse> + Send,
{
    async fn load<U>(self, conn: &Connection) -> QueryResult<Vec<U>>
    where
        U: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromNativeBlock + Send,
    {
        conn.load(self).await
    }

    async fn first<U>(self, conn: &Connection) -> QueryResult<U>
    where
        U: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromNativeBlock + Send,
    {
        conn.load(self).await?.into_iter().next().ok_or(Error::NotFound)
    }

    async fn get_result<U>(self, conn: &Connection) -> QueryResult<Option<U>>
    where
        U: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromNativeBlock + Send,
    {
        Ok(conn.load(self).await?.into_iter().next())
    }

    async fn execute(self, conn: &Connection) -> QueryResult<()> {
        conn.execute_query(self).await
    }
}

// =============================================================================
// Optimized Insert DSL - Uses binary formats instead of SQL text
// =============================================================================

use crate::core::query_builder::{InsertStatement, Insertable};
use crate::core::query_source::Table;

/// Extension trait for optimized insert execution.
///
/// This trait provides `execute_optimized()` which uses binary formats
/// instead of SQL text for maximum insert performance.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::prelude::*;
///
/// // Uses optimized binary insert (RowBinary for HTTP, Block for Native)
/// insert_into(users::table)
///     .values(new_users.as_slice())
///     .execute_optimized(&conn)
///     .await?;
/// ```
#[allow(async_fn_in_trait)]
pub trait OptimizedInsertDsl: Sized {
    /// Execute the insert using optimized binary format.
    ///
    /// - **HTTP**: Uses RowBinary format via inserter
    /// - **Native**: Uses native Block format
    async fn execute_optimized(self, conn: &Connection) -> QueryResult<()>;
}

// HTTP implementation for slice of rows
#[cfg(feature = "http")]
impl<T, R> OptimizedInsertDsl for InsertStatement<T, &[R]>
where
    T: Table,
    R: Insertable<T> + clickhouse::RowOwned + clickhouse::RowWrite + Send + Sync,
{
    async fn execute_optimized(self, conn: &Connection) -> QueryResult<()> {
        let rows: &[R] = self.values_ref();
        if rows.is_empty() {
            return Ok(());
        }

        conn.insert_rows(T::table_name(), rows).await
    }
}

// Native-only implementation for slice of rows
#[cfg(all(feature = "native", not(feature = "http")))]
impl<T, R> OptimizedInsertDsl for InsertStatement<T, &[R]>
where
    T: Table,
    R: Insertable<T> + crate::native::ToNativeBlock + Send + Sync,
{
    async fn execute_optimized(self, conn: &Connection) -> QueryResult<()> {
        let rows: &[R] = self.values_ref();
        if rows.is_empty() {
            return Ok(());
        }

        conn.insert_native(T::table_name(), rows).await
    }
}
