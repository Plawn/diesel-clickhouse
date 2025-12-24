//! SQL building utilities for query fragments.
//!
//! This module provides the common functionality for building SQL strings
//! from `QueryFragment` types, shared across HTTP and Native backends.

use crate::backend::{BindCollector, ClickHouse, GenericBindCollector, GenericQueryBuilder, QueryBuilder};
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

// Re-export for convenience
pub use crate::backend::BindableValue;

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
    Ok((builder.finish(), collector.bindable_values().to_vec()))
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
}
