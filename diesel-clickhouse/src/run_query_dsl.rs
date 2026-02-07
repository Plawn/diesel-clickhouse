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
//! #[clickhouse_row]
//! #[derive(Debug)]
//! struct User {
//!     id: u64,
//!     name: String,
//! }
//!
//! // Idiomatic Diesel style with compile-time type verification:
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
use crate::core::deserialize::Queryable;
use crate::core::query_builder::{QueryFragment, QueryOutputType};
use crate::core::result::{Error, QueryResult};
use crate::Connection;

// =============================================================================
// Backend-specific trait bounds (factorized)
// =============================================================================

/// Trait alias for row types that can be loaded from the database.
///
/// This trait combines all necessary bounds for a type to be deserialized
/// from query results, abstracting over the backend-specific requirements.
#[cfg(all(feature = "http", not(feature = "native")))]
pub trait LoadableRow: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send {}

#[cfg(all(feature = "http", not(feature = "native")))]
impl<T> LoadableRow for T where T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + Send {}

#[cfg(all(feature = "native", not(feature = "http")))]
pub trait LoadableRow: crate::native::FromNativeBlock + Send {}

#[cfg(all(feature = "native", not(feature = "http")))]
impl<T> LoadableRow for T where T: crate::native::FromNativeBlock + Send {}

#[cfg(all(feature = "http", feature = "native"))]
pub trait LoadableRow: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromNativeBlock + Send {}

#[cfg(all(feature = "http", feature = "native"))]
impl<T> LoadableRow for T where T: clickhouse::Row + clickhouse::RowOwned + clickhouse::RowRead + crate::native::FromNativeBlock + Send {}

// =============================================================================
// RunQueryDsl trait definition
// =============================================================================

/// Extension trait for executing queries in idiomatic Diesel style.
///
/// This trait is automatically implemented for all types that implement
/// `QueryFragment<ClickHouse> + QueryOutputType`, allowing you to call `.load()`, `.first()`,
/// `.get_result()` directly on queries.
///
/// Row types must be marked with `#[clickhouse_row]` for compile-time type verification.
#[allow(async_fn_in_trait)]
pub trait RunQueryDsl: Sized + QueryOutputType {
    /// Execute the query and load all results with compile-time type verification.
    ///
    /// The row type must implement `Queryable<Self::SqlType>` to ensure
    /// the struct matches the query's output columns at compile time.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[clickhouse_row]
    /// struct User { id: u64, name: String }
    ///
    /// let users: Vec<User> = users::table
    ///     .filter(users::active.eq(true))
    ///     .load(&conn)
    ///     .await?;
    /// ```
    async fn load<U>(self, conn: &Connection) -> QueryResult<Vec<U>>
    where
        U: Queryable<Self::SqlType> + LoadableRow;

    /// Execute the query and return the first result with compile-time type verification.
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
    async fn first<U>(self, conn: &Connection) -> QueryResult<U>
    where
        U: Queryable<Self::SqlType> + LoadableRow;

    /// Execute the query and return an optional result with compile-time type verification.
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
    async fn get_result<U>(self, conn: &Connection) -> QueryResult<Option<U>>
    where
        U: Queryable<Self::SqlType> + LoadableRow;
}

// =============================================================================
// RunQueryDsl implementation (single, unified)
// =============================================================================

impl<T> RunQueryDsl for T
where
    T: QueryFragment<ClickHouse> + QueryOutputType + Send,
{
    async fn load<U>(self, conn: &Connection) -> QueryResult<Vec<U>>
    where
        U: Queryable<Self::SqlType> + LoadableRow,
    {
        conn.load(self).await
    }

    async fn first<U>(self, conn: &Connection) -> QueryResult<U>
    where
        U: Queryable<Self::SqlType> + LoadableRow,
    {
        conn.load_one(self).await
    }

    async fn get_result<U>(self, conn: &Connection) -> QueryResult<Option<U>>
    where
        U: Queryable<Self::SqlType> + LoadableRow,
    {
        conn.load_optional(self).await
    }
}

// =============================================================================
// ExecuteDsl trait
// =============================================================================

/// Extension trait for executing mutation statements (INSERT, UPDATE, DELETE).
///
/// This trait is automatically implemented for all types that implement
/// `QueryFragment<ClickHouse>`, allowing you to call `.execute()` on mutations.
///
/// Unlike `RunQueryDsl`, this trait does not require `QueryOutputType` because
/// mutation statements don't return rows that need type verification.
#[allow(async_fn_in_trait)]
pub trait ExecuteDsl: Sized {
    /// Execute the statement (for INSERT, UPDATE, DELETE).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// update(users::table)
    ///     .set(users::name.eq("New Name"))
    ///     .filter(users::id.eq(1))
    ///     .execute(&conn)
    ///     .await?;
    /// ```
    async fn execute(self, conn: &Connection) -> QueryResult<()>;
}

impl<T> ExecuteDsl for T
where
    T: QueryFragment<ClickHouse> + Send,
{
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
/// This trait is automatically implemented for `InsertStatement` and provides
/// an `insert()` method that uses binary formats instead of SQL text.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::prelude::*;
///
/// // Uses optimized binary insert (RowBinary for HTTP, Block for Native)
/// insert_into(users::table)
///     .values(new_users.as_slice())
///     .insert(&conn)
///     .await?;
/// ```
#[allow(async_fn_in_trait)]
pub trait InsertDsl: Sized {
    /// Execute the insert using optimized binary format.
    ///
    /// - **HTTP**: Uses RowBinary format via inserter
    /// - **Native**: Uses native Block format
    ///
    /// This method automatically selects the optimal insert mechanism
    /// based on the connection type. Always prefer this over `.execute()`
    /// for INSERT statements.
    async fn insert(self, conn: &Connection) -> QueryResult<()>;
}

// =============================================================================
// InsertDsl - Backend-specific trait bounds (factorized)
// =============================================================================

/// Trait alias for row types that can be inserted into the database.
#[cfg(all(feature = "http", not(feature = "native")))]
pub trait InsertableRow<T: Table>: Insertable<T> + clickhouse::RowOwned + clickhouse::RowWrite + Send + Sync {}

#[cfg(all(feature = "http", not(feature = "native")))]
impl<T: Table, R> InsertableRow<T> for R where R: Insertable<T> + clickhouse::RowOwned + clickhouse::RowWrite + Send + Sync {}

#[cfg(all(feature = "native", not(feature = "http")))]
pub trait InsertableRow<T: Table>: Insertable<T> + crate::native::ToNativeBlock + Send + Sync {}

#[cfg(all(feature = "native", not(feature = "http")))]
impl<T: Table, R> InsertableRow<T> for R where R: Insertable<T> + crate::native::ToNativeBlock + Send + Sync {}

#[cfg(all(feature = "http", feature = "native"))]
pub trait InsertableRow<T: Table>: Insertable<T> + clickhouse::RowOwned + clickhouse::RowWrite + crate::native::ToNativeBlock + Send + Sync {}

#[cfg(all(feature = "http", feature = "native"))]
impl<T: Table, R> InsertableRow<T> for R where R: Insertable<T> + clickhouse::RowOwned + clickhouse::RowWrite + crate::native::ToNativeBlock + Send + Sync {}

// =============================================================================
// Backend-specific insert helpers (factorized logic)
// =============================================================================

/// Insert rows via HTTP backend using RowBinary format.
#[cfg(feature = "http")]
async fn insert_via_http<T, R>(
    http_conn: &crate::http::ClickHouseConnection,
    rows: &[R],
) -> QueryResult<()>
where
    T: Table,
    R: clickhouse::RowOwned + clickhouse::RowWrite + Send + Sync,
{
    let mut inserter = http_conn
        .client()
        .insert::<R>(T::table_name())
        .await
        .map_err(Error::query_from)?;

    for row in rows {
        inserter.write(row).await.map_err(Error::query_from)?;
    }

    inserter.end().await.map_err(Error::query_from)?;
    Ok(())
}

/// Insert rows via Native backend using Block format or SQL fallback.
#[cfg(feature = "native")]
async fn insert_via_native<T, R>(
    native_conn: &crate::native::NativeConnection,
    rows: &[R],
) -> QueryResult<()>
where
    T: Table + Default,
    R: Insertable<T> + crate::native::ToNativeBlock + Send + Sync,
{
    // Use SQL insert for types with JSON fields (Block API doesn't support JSON)
    if R::REQUIRES_SQL_INSERT {
        use crate::core::query_builder::insert_into;
        let insert = insert_into(T::default()).values(rows);
        let compiled = crate::core::sql_builder::compile_query(&insert)?;
        let sql = compiled.to_interpolated_sql()?;
        native_conn.execute_raw(&sql).await
    } else {
        native_conn.insert_native(T::table_name(), rows).await
    }
}

// =============================================================================
// InsertDsl implementation (unified, delegates to backend helpers)
// =============================================================================

// HTTP-only: Table doesn't need Default
#[cfg(all(feature = "http", not(feature = "native")))]
impl<T, R> InsertDsl for InsertStatement<T, &[R]>
where
    T: Table,
    R: InsertableRow<T>,
{
    async fn insert(self, conn: &Connection) -> QueryResult<()> {
        let rows: &[R] = self.values_ref();
        if rows.is_empty() {
            return Ok(());
        }

        let Connection::Http(http_conn) = conn;
        insert_via_http::<T, R>(http_conn, rows).await
    }
}

// Native-only: Table needs Default for SQL fallback
#[cfg(all(feature = "native", not(feature = "http")))]
impl<T, R> InsertDsl for InsertStatement<T, &[R]>
where
    T: Table + Default,
    R: InsertableRow<T>,
{
    async fn insert(self, conn: &Connection) -> QueryResult<()> {
        let rows: &[R] = self.values_ref();
        if rows.is_empty() {
            return Ok(());
        }

        let Connection::Native(native_conn) = conn;
        insert_via_native::<T, R>(native_conn, rows).await
    }
}

// Both HTTP and Native: delegates to appropriate helper
#[cfg(all(feature = "http", feature = "native"))]
impl<T, R> InsertDsl for InsertStatement<T, &[R]>
where
    T: Table + Default,
    R: InsertableRow<T>,
{
    async fn insert(self, conn: &Connection) -> QueryResult<()> {
        let rows: &[R] = self.values_ref();
        if rows.is_empty() {
            return Ok(());
        }

        match conn {
            Connection::Http(http_conn) => insert_via_http::<T, R>(http_conn, rows).await,
            Connection::Native(native_conn) => insert_via_native::<T, R>(native_conn, rows).await,
        }
    }
}
