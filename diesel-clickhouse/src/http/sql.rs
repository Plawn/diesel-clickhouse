//! SQL building utilities for HTTP backend.
//!
//! This module provides HTTP-specific query compilation, adding the `bind_to()`
//! method for applying bindings to `clickhouse::query::Query` objects.

use crate::core::backend::ClickHouse;
use crate::core::query_builder::QueryFragment;
use crate::core::result::QueryResult;

// Re-export from core for backward compatibility
pub use crate::core::sql_builder::{build_sql, BindableValue, CompiledQuery, compile_query};

/// Type alias for backward compatibility.
///
/// `NativeCompiledQuery` is now an alias for the unified `CompiledQuery` type
/// from `diesel-clickhouse-core`.
pub type NativeCompiledQuery = CompiledQuery;

// =============================================================================
// HTTP-specific extensions for CompiledQuery
// =============================================================================

/// HTTP-specific extensions for `CompiledQuery`.
///
/// These methods are only available in the HTTP backend and provide
/// integration with the `clickhouse` crate's native binding API.
pub trait CompiledQueryExt {
    /// Apply all bindings to a clickhouse Query object.
    ///
    /// This uses the clickhouse crate's native `.bind()` mechanism which
    /// enables query plan caching on the server.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use diesel_clickhouse::http::CompiledQueryExt;
    ///
    /// let compiled = compile_query(&users::table.filter(users::id.eq(42)))?;
    /// let query = compiled.bind_to(client.query(&compiled.sql));
    /// let rows: Vec<User> = query.fetch_all().await?;
    /// ```
    fn bind_to(&self, query: clickhouse::query::Query) -> clickhouse::query::Query;
}

impl CompiledQueryExt for CompiledQuery {
    fn bind_to(&self, mut query: clickhouse::query::Query) -> clickhouse::query::Query {
        for binding in &self.bindings {
            query = query.bind(binding);
        }
        query
    }
}

/// Build SQL with native bindable values from a QueryFragment.
///
/// This is the recommended way to build queries for execution:
/// - Returns a `CompiledQuery` with SQL and typed bindings
/// - Use `.bind_to()` to apply bindings to a clickhouse Query
/// - Enables query plan caching on the ClickHouse server
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse::http::CompiledQueryExt;
///
/// let compiled = build_sql_native(&users::table.filter(users::active.eq(true)))?;
/// let query = compiled.bind_to(client.query(&compiled.sql));
/// let rows: Vec<User> = query.fetch_all().await?;
/// ```
pub fn build_sql_native<T: QueryFragment<ClickHouse> + ?Sized>(fragment: &T) -> QueryResult<CompiledQuery> {
    compile_query(fragment)
}
