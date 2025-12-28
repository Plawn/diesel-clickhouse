//! Test utilities for diesel-clickhouse.
//!
//! This module provides shared helpers for testing query building
//! across the codebase. It eliminates duplication of common test patterns.

use crate::backend::{BindCollector, ClickHouse, GenericBindCollector, GenericQueryBuilder, QueryBuilder};
use crate::query_builder::{AstPass, QueryFragment};
use crate::result::QueryResult;

/// Build SQL from a QueryFragment with bindings inlined for test assertions.
///
/// This is useful for testing because it produces deterministic SQL strings
/// that can be directly compared with expected values.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse_core::test_utils::build_sql_inlined;
///
/// let stmt = SelectStatement::new(TestTable("users"));
/// assert_eq!(build_sql_inlined(&stmt), "SELECT * FROM `users`");
/// ```
pub fn build_sql_inlined<T: QueryFragment<ClickHouse>>(fragment: &T) -> String {
    let mut builder = GenericQueryBuilder::default();
    let mut collector = GenericBindCollector::default();
    let pass: AstPass<'_, '_, ClickHouse> = AstPass::new(&mut builder, &mut collector);

    // Use unwrap here since this is test-only code
    #[allow(clippy::unwrap_used)]
    fragment.walk_ast(pass).unwrap();

    // Inline bindings into the SQL for easier test assertions
    // GenericQueryBuilder uses '?' as placeholder
    let mut sql = builder.finish();
    for binding in collector.bindable_values().iter().rev() {
        if let Some(pos) = sql.rfind('?') {
            sql.replace_range(pos..pos + 1, &binding.sql_literal());
        }
    }
    sql
}

/// A simple test table that can be parameterized with a table name.
///
/// # Example
///
/// ```rust,ignore
/// use diesel_clickhouse_core::test_utils::TestTable;
///
/// let users = TestTable("users");
/// let events = TestTable("events");
/// ```
pub struct TestTable(pub &'static str);

impl QueryFragment<ClickHouse> for TestTable {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, ClickHouse>) -> QueryResult<()> {
        pass.push_identifier(self.0);
        Ok(())
    }
}

/// A test table that outputs raw SQL without identifier quoting.
///
/// Useful when you need to test with literal SQL fragments.
pub struct RawTable(pub &'static str);

impl QueryFragment<ClickHouse> for RawTable {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, ClickHouse>) -> QueryResult<()> {
        pass.push_sql(self.0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_builder::SelectStatement;

    #[test]
    fn test_build_sql_inlined_simple() {
        let stmt = SelectStatement::new(TestTable("users"));
        assert_eq!(build_sql_inlined(&stmt), "SELECT * FROM `users`");
    }

    #[test]
    fn test_build_sql_inlined_with_raw_table() {
        let stmt = SelectStatement::new(RawTable("my_database.users"));
        assert_eq!(build_sql_inlined(&stmt), "SELECT * FROM my_database.users");
    }

    #[test]
    fn test_table_names() {
        assert_eq!(build_sql_inlined(&TestTable("events")), "`events`");
        assert_eq!(build_sql_inlined(&TestTable("user_actions")), "`user_actions`");
    }
}
