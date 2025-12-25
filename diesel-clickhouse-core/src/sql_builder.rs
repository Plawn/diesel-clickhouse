//! SQL building utilities for query fragments.
//!
//! This module provides the common functionality for building SQL strings
//! from `QueryFragment` types, shared across HTTP and Native backends.
//!
//! # Architecture
//!
//! SQL compilation follows a unified path:
//!
//! 1. `compile_query(fragment)` → `CompiledQuery { sql, bindings }`
//! 2. Backend-specific conversion:
//!    - HTTP: Uses native `.bind()` calls for query plan caching
//!    - Native: Uses `.to_interpolated_sql()` to inline literals
//!
//! # Example
//!
//! ```rust,ignore
//! use diesel_clickhouse_core::sql_builder::{compile_query, CompiledQuery};
//!
//! let compiled = compile_query(&users::table.filter(users::id.eq(42)))?;
//! // compiled.sql: "SELECT * FROM `users` WHERE `id` = ?"
//! // compiled.bindings: [BindableValue::UInt64(42)]
//!
//! // For backends without native binding support:
//! let sql = compiled.to_interpolated_sql()?;
//! // sql: "SELECT * FROM `users` WHERE `id` = 42"
//! ```

use std::borrow::Cow;

use crate::backend::{BindCollector, ClickHouse, GenericBindCollector, GenericQueryBuilder, QueryBuilder};
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::{Error, QueryResult};

// Re-export for convenience
pub use crate::backend::BindableValue;

// =============================================================================
// CompiledQuery - Unified query compilation output
// =============================================================================

/// A compiled query with SQL placeholders and typed bindable values.
///
/// This is the unified output of query compilation, usable by all backends.
/// Each backend applies the bindings differently:
///
/// - **HTTP backend**: Uses native `.bind()` calls for query plan caching
/// - **Native backend**: Interpolates values into SQL via `.to_interpolated_sql()`
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse_core::sql_builder::compile_query;
///
/// let compiled = compile_query(&users::table.filter(users::id.eq(42)))?;
/// assert_eq!(compiled.sql, "SELECT * FROM `users` WHERE `id` = ?");
/// assert_eq!(compiled.param_count(), 1);
///
/// // Convert to fully-interpolated SQL (for backends without binding support):
/// let sql = compiled.to_interpolated_sql()?;
/// assert_eq!(sql, "SELECT * FROM `users` WHERE `id` = 42");
/// ```
#[derive(Debug, Clone)]
pub struct CompiledQuery {
    /// The SQL string with `?` placeholders.
    pub sql: String,
    /// The collected bindable values.
    pub bindings: Vec<BindableValue>,
}

impl CompiledQuery {
    /// Create a new compiled query.
    pub fn new(sql: String, bindings: Vec<BindableValue>) -> Self {
        Self { sql, bindings }
    }

    /// Get the number of bind parameters.
    pub fn param_count(&self) -> usize {
        self.bindings.len()
    }

    /// Check if there are any bind parameters.
    pub fn has_bindings(&self) -> bool {
        !self.bindings.is_empty()
    }

    /// Convert to fully-interpolated SQL (for backends without native binding).
    ///
    /// Replaces all `?` placeholders with their literal SQL values.
    /// This is required for backends like `clickhouse-rs` that don't support
    /// native parameter binding.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let compiled = compile_query(&users::table.filter(users::name.eq("Alice")))?;
    /// let sql = compiled.to_interpolated_sql()?;
    /// // sql: "SELECT * FROM `users` WHERE `name` = 'Alice'"
    /// ```
    pub fn to_interpolated_sql(&self) -> QueryResult<String> {
        interpolate_bindings(&self.sql, &self.bindings)
    }
}

/// Build a compiled query from a QueryFragment.
///
/// This is the primary entry point for query compilation. Returns a
/// `CompiledQuery` containing the SQL with `?` placeholders and the
/// collected bind values.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse_core::sql_builder::compile_query;
///
/// let compiled = compile_query(&users::table.filter(users::active.eq(true)))?;
/// println!("SQL: {}", compiled.sql);
/// println!("Bindings: {:?}", compiled.bindings);
/// ```
pub fn compile_query<T: QueryFragment<ClickHouse> + ?Sized>(
    fragment: &T,
) -> QueryResult<CompiledQuery> {
    let (sql, bindings) = build_sql_with_bindings(fragment)?;
    Ok(CompiledQuery::new(sql, bindings))
}

/// Replace `?` placeholders in SQL with actual bind values.
///
/// Optimized to avoid per-binding allocations by writing directly to the output buffer.
fn interpolate_bindings(sql: &str, bindings: &[BindableValue]) -> QueryResult<String> {
    // Estimate capacity: original SQL + ~12 bytes per binding (average literal size)
    let mut result = String::with_capacity(sql.len() + bindings.len() * 12);
    let mut bindings_iter = bindings.iter();

    // Split by '?' and interleave with binding literals
    let mut segments = sql.split('?');

    // First segment (before any '?')
    if let Some(first) = segments.next() {
        result.push_str(first);
    }

    // Remaining segments - each is preceded by a '?' that needs a binding
    for segment in segments {
        match bindings_iter.next() {
            Some(binding) => binding.write_sql_literal(&mut result),
            None => {
                return Err(Error::QueryError(Cow::Borrowed(
                    "Not enough bind values for query placeholders"
                )));
            }
        }
        result.push_str(segment);
    }

    // Check for unused bindings
    if bindings_iter.next().is_some() {
        return Err(Error::QueryError(Cow::Borrowed(
            "Too many bind values for query placeholders"
        )));
    }

    Ok(result)
}

// =============================================================================
// Legacy API (for backward compatibility)
// =============================================================================

/// Build SQL from a QueryFragment.
///
/// Returns the SQL string with `?` placeholders for parameters.
/// This is a lightweight function for display/debugging purposes.
///
/// For actual query execution, use the backend-specific methods which apply
/// appropriate parameter binding.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse_core::sql_builder::build_sql;
///
/// let sql = build_sql(&users::table.filter(users::id.eq(42)))?;
/// // Returns: "SELECT * FROM `users` WHERE `id` = ?"
/// ```
pub fn build_sql<T: QueryFragment<ClickHouse> + ?Sized>(fragment: &T) -> QueryResult<String> {
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
    fragment.walk_ast(pass)?;
    Ok(builder.finish())
}

/// Build SQL with collected bind values from a QueryFragment.
///
/// Returns both the SQL string with `?` placeholders and the collected
/// `BindableValue` instances for parameter binding.
///
/// This is the foundation for backend-specific query compilation:
/// - HTTP backend uses the bindings with native `.bind()` calls
/// - Native backend interpolates the bindings into the SQL string
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse_core::sql_builder::build_sql_with_bindings;
///
/// let (sql, bindings) = build_sql_with_bindings(&users::table.filter(users::id.eq(42)))?;
/// // sql: "SELECT * FROM `users` WHERE `id` = ?"
/// // bindings: [BindableValue::UInt64(42)]
/// ```
pub fn build_sql_with_bindings<T: QueryFragment<ClickHouse> + ?Sized>(
    fragment: &T,
) -> QueryResult<(String, Vec<BindableValue>)> {
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);
    fragment.walk_ast(pass)?;
    // Use into_bindable_values() to take ownership instead of cloning
    Ok((builder.finish(), collector.into_bindable_values()))
}

/// Extension trait for query fragments to convert to SQL string.
///
/// This is automatically implemented for all types that implement
/// `QueryFragment<ClickHouse>`.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse_core::sql_builder::ToSqlString;
///
/// let sql = users::table.filter(users::active.eq(true)).to_sql_string()?;
/// ```
pub trait ToSqlString: QueryFragment<ClickHouse> {
    /// Convert to SQL string with `?` placeholders.
    ///
    /// Returns an error if the query fragment fails to produce valid SQL.
    fn to_sql_string(&self) -> QueryResult<String> {
        build_sql(self)
    }

    /// Convert to SQL string with collected bind values.
    ///
    /// Returns a tuple of (sql_with_placeholders, bind_values).
    fn to_sql_with_bindings(&self) -> QueryResult<(String, Vec<BindableValue>)> {
        build_sql_with_bindings(self)
    }
}

/// Blanket implementation for all QueryFragment types.
impl<T: QueryFragment<ClickHouse>> ToSqlString for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_builder::SelectStatement;

    // Simple table wrapper for testing
    struct TestTable;

    impl QueryFragment<ClickHouse> for TestTable {
        fn walk_ast<'b>(
            &'b self,
            mut pass: AstPass<'_, 'b, ClickHouse>,
        ) -> QueryResult<()> {
            pass.push_sql("test_table");
            Ok(())
        }
    }

    #[test]
    fn test_build_sql() {
        let query = SelectStatement::new(TestTable);
        let result = build_sql(&query).expect("failed to build SQL");
        assert_eq!(result, "SELECT * FROM test_table");
    }

    #[test]
    fn test_to_sql_string_trait() {
        let query = SelectStatement::new(TestTable);
        let result = query.to_sql_string().expect("failed to build SQL");
        assert_eq!(result, "SELECT * FROM test_table");
    }

    #[test]
    fn test_build_sql_with_bindings() {
        let query = SelectStatement::new(TestTable);
        let (sql, bindings) = build_sql_with_bindings(&query).expect("failed to build SQL");
        assert_eq!(sql, "SELECT * FROM test_table");
        assert!(bindings.is_empty());
    }

    // =========================================================================
    // CompiledQuery tests
    // =========================================================================

    #[test]
    fn test_compile_query() {
        let query = SelectStatement::new(TestTable);
        let compiled = compile_query(&query).expect("failed to compile query");
        assert_eq!(compiled.sql, "SELECT * FROM test_table");
        assert!(compiled.bindings.is_empty());
        assert_eq!(compiled.param_count(), 0);
        assert!(!compiled.has_bindings());
    }

    #[test]
    fn test_compiled_query_to_interpolated_sql_no_bindings() {
        let compiled = CompiledQuery::new(
            "SELECT * FROM test_table".to_string(),
            vec![],
        );
        let sql = compiled.to_interpolated_sql().expect("interpolation failed");
        assert_eq!(sql, "SELECT * FROM test_table");
    }

    #[test]
    fn test_compiled_query_to_interpolated_sql_with_bindings() {
        let compiled = CompiledQuery::new(
            "SELECT * FROM users WHERE id = ? AND name = ?".to_string(),
            vec![
                BindableValue::U64(42),
                BindableValue::owned_string("Alice".to_string()),
            ],
        );
        let sql = compiled.to_interpolated_sql().expect("interpolation failed");
        assert_eq!(sql, "SELECT * FROM users WHERE id = 42 AND name = 'Alice'");
    }

    #[test]
    fn test_interpolate_bindings_not_enough_values() {
        let result = interpolate_bindings(
            "SELECT * FROM users WHERE id = ? AND name = ?",
            &[BindableValue::U64(42)],
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Not enough bind values"));
    }

    #[test]
    fn test_interpolate_bindings_too_many_values() {
        let result = interpolate_bindings(
            "SELECT * FROM users WHERE id = ?",
            &[BindableValue::U64(42), BindableValue::U64(100)],
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Too many bind values"));
    }

    #[test]
    fn test_interpolate_bindings_string_escaping() {
        let compiled = CompiledQuery::new(
            "SELECT * FROM users WHERE name = ?".to_string(),
            vec![BindableValue::owned_string("O'Brien".to_string())],
        );
        let sql = compiled.to_interpolated_sql().expect("interpolation failed");
        assert_eq!(sql, "SELECT * FROM users WHERE name = 'O''Brien'");
    }

    #[test]
    fn test_interpolate_bindings_all_types() {
        let compiled = CompiledQuery::new(
            "INSERT INTO test VALUES (?, ?, ?, ?, ?)".to_string(),
            vec![
                BindableValue::U8(255),
                BindableValue::I64(-42),
                BindableValue::F64(3.14),
                BindableValue::Bool(true),
                BindableValue::static_str("test"),
            ],
        );
        let sql = compiled.to_interpolated_sql().expect("interpolation failed");
        assert_eq!(sql, "INSERT INTO test VALUES (255, -42, 3.14, true, 'test')");
    }
}
